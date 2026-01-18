//! MongoDB query safety classification.
//!
//! Used to decide whether a query is read-only or potentially mutating.

use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MongoQueryClass {
    Read,
    Mutation,
    Unknown,
}

pub fn classify(query: &str) -> MongoQueryClass {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return MongoQueryClass::Unknown;
    }

    if trimmed.starts_with('{') {
        match serde_json::from_str::<JsonValue>(trimmed) {
            Ok(value) => return classify_json(&value),
            Err(_) => return MongoQueryClass::Unknown,
        }
    }

    classify_shell(trimmed)
}

fn classify_json(value: &JsonValue) -> MongoQueryClass {
    let operation = value.get("operation").and_then(|v| v.as_str());
    match operation {
        None => MongoQueryClass::Read,
        Some(op) => classify_operation(op, value),
    }
}

fn classify_operation(op_raw: &str, value: &JsonValue) -> MongoQueryClass {
    let op = normalize_op(op_raw);

    if is_read_op(&op) {
        if op == "aggregate" && aggregate_writes(value) {
            return MongoQueryClass::Mutation;
        }
        return MongoQueryClass::Read;
    }

    if is_mutation_op(&op) {
        return MongoQueryClass::Mutation;
    }

    MongoQueryClass::Unknown
}

fn aggregate_writes(value: &JsonValue) -> bool {
    let pipeline = value.get("pipeline").and_then(|v| v.as_array());
    if let Some(stages) = pipeline {
        for stage in stages {
            if stage.get("$out").is_some() || stage.get("$merge").is_some() {
                return true;
            }
        }
    }
    false
}

fn normalize_op(op: &str) -> String {
    op.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn is_read_op(op: &str) -> bool {
    matches!(
        op,
        "find"
            | "findone"
            | "aggregate"
            | "count"
            | "countdocuments"
            | "estimateddocumentcount"
            | "distinct"
            | "listcollections"
            | "collstats"
            | "dbstats"
    )
}

fn is_mutation_op(op: &str) -> bool {
    matches!(
        op,
        "insert"
            | "insertone"
            | "insertmany"
            | "update"
            | "updateone"
            | "updatemany"
            | "replaceone"
            | "delete"
            | "deleteone"
            | "deletemany"
            | "remove"
            | "bulkwrite"
            | "findoneandupdate"
            | "findoneanddelete"
            | "findoneandreplace"
            | "createcollection"
            | "drop"
            | "dropcollection"
            | "dropdatabase"
            | "renamecollection"
            | "mapreduce"
            | "findandmodify"
    )
}

fn classify_shell(query: &str) -> MongoQueryClass {
    let lowered = query.to_ascii_lowercase();
    let compact: String = lowered.split_whitespace().collect();

    let mutation_patterns = [
        ".findoneandupdate(",
        ".findoneanddelete(",
        ".findoneandreplace(",
        ".insertone(",
        ".insertmany(",
        ".insert(",
        ".updateone(",
        ".updatemany(",
        ".update(",
        ".replaceone(",
        ".deleteone(",
        ".deletemany(",
        ".delete(",
        ".remove(",
        ".createcollection(",
        ".drop(",
        ".dropdatabase(",
        ".bulkwrite(",
        ".renamecollection(",
        ".findandmodify(",
        ".mapreduce(",
    ];

    if mutation_patterns.iter().any(|pattern| compact.contains(pattern)) {
        return MongoQueryClass::Mutation;
    }

    let read_patterns = [
        ".findone(",
        ".find(",
        ".aggregate(",
        ".countdocuments(",
        ".estimateddocumentcount(",
        ".count(",
        ".distinct(",
        ".listcollections(",
        ".collstats(",
        ".dbstats(",
    ];

    if read_patterns.iter().any(|pattern| compact.contains(pattern)) {
        if compact.contains(".aggregate(")
            && (compact.contains("$out") || compact.contains("$merge"))
        {
            return MongoQueryClass::Mutation;
        }
        return MongoQueryClass::Read;
    }

    if looks_like_collection_reference(&compact) {
        return MongoQueryClass::Read;
    }

    MongoQueryClass::Unknown
}

fn looks_like_collection_reference(compact: &str) -> bool {
    if compact.contains('(') {
        return false;
    }

    let mut dots = 0;
    for c in compact.chars() {
        if c == '.' {
            dots += 1;
        }
    }

    dots == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_insert_is_mutation() {
        let query = r#"{"operation":"insert","database":"db","collection":"col","document":{}}"#;
        assert_eq!(classify(query), MongoQueryClass::Mutation);
    }

    #[test]
    fn json_find_without_operation_is_read() {
        let query = r#"{"database":"db","collection":"col","query":{}}"#;
        assert_eq!(classify(query), MongoQueryClass::Read);
    }

    #[test]
    fn json_aggregate_with_out_is_mutation() {
        let query = r#"{"operation":"aggregate","pipeline":[{"$out":"archive"}]}"#;
        assert_eq!(classify(query), MongoQueryClass::Mutation);
    }

    #[test]
    fn shell_find_is_read() {
        let query = "db.users.find({})";
        assert_eq!(classify(query), MongoQueryClass::Read);
    }

    #[test]
    fn shell_update_is_mutation() {
        let query = "db.users.updateOne({},{})";
        assert_eq!(classify(query), MongoQueryClass::Mutation);
    }

    #[test]
    fn shell_getcollection_without_method_is_unknown() {
        let query = "db.getCollection('users')";
        assert_eq!(classify(query), MongoQueryClass::Unknown);
    }
}
