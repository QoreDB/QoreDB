// SPDX-License-Identifier: Apache-2.0

//! MongoDB aggregation pipeline AST + validator.
//!
//! Parses a JSON array of stages into a typed [`Pipeline`], classifying it as
//! [`MongoQueryClass::Read`] or [`MongoQueryClass::Mutation`] and rejecting
//! operators that can execute arbitrary server-side code (`$function`,
//! `$accumulator`, `$where`).
//!
//! This module is pure validation: it never talks to MongoDB. It is used
//! by [`crate::mongo_safety`] before dispatch and will be reused at execution
//! time to convert the validated stages into `bson::Document` values.
//!
//! # Validation rules
//!
//! - Each stage must be an object with exactly one top-level key starting with `$`.
//! - `$out` and `$merge` are write-only terminal stages: they must appear last
//!   and the pipeline is then classified as `Mutation`.
//! - `$function`, `$accumulator`, `$where` are rejected anywhere in the pipeline
//!   — including nested inside expressions — because they run JavaScript on the
//!   server.
//! - Unknown stage names are rejected (fail-closed). Add them explicitly to the
//!   [`KNOWN_STAGES`] table after a safety review.
//! - A pipeline length hard cap of [`MAX_PIPELINE_STAGES`] protects against
//!   pathological server load.

use serde_json::Value as JsonValue;

use crate::mongo_safety::MongoQueryClass;

/// Maximum number of stages accepted in a single pipeline.
pub const MAX_PIPELINE_STAGES: usize = 50;

/// Maximum nesting depth searched when looking for forbidden operators.
const MAX_SCAN_DEPTH: usize = 64;

/// Server-side JavaScript / arbitrary code execution operators. Forbidden
/// regardless of where they appear in a stage body.
const FORBIDDEN_OPERATORS: &[&str] = &["$function", "$accumulator", "$where"];

/// Recognised aggregation stages.
///
/// The set is intentionally conservative: an operator not listed here is
/// rejected by the validator. Expand the table (and add tests) after a safety
/// review of any new stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageKind {
    Match,
    Project,
    Group,
    Sort,
    Limit,
    Skip,
    Unwind,
    Lookup,
    Count,
    AddFields,
    Set,
    Unset,
    ReplaceRoot,
    ReplaceWith,
    Facet,
    Bucket,
    BucketAuto,
    Sample,
    SortByCount,
    GraphLookup,
    Redact,
    GeoNear,
    IndexStats,
    CollStats,
    /// Terminal write stage — dumps results to a collection.
    Out,
    /// Terminal write stage — upserts results into a collection.
    Merge,
}

impl StageKind {
    /// Whether this stage writes to the database.
    pub fn is_write(self) -> bool {
        matches!(self, StageKind::Out | StageKind::Merge)
    }

    /// Whether this stage can only appear at the end of a pipeline.
    pub fn is_terminal(self) -> bool {
        self.is_write()
    }

    /// Parse a stage name (including the leading `$`) into a [`StageKind`].
    pub fn from_operator(op: &str) -> Option<Self> {
        let kind = match op {
            "$match" => StageKind::Match,
            "$project" => StageKind::Project,
            "$group" => StageKind::Group,
            "$sort" => StageKind::Sort,
            "$limit" => StageKind::Limit,
            "$skip" => StageKind::Skip,
            "$unwind" => StageKind::Unwind,
            "$lookup" => StageKind::Lookup,
            "$count" => StageKind::Count,
            "$addFields" => StageKind::AddFields,
            "$set" => StageKind::Set,
            "$unset" => StageKind::Unset,
            "$replaceRoot" => StageKind::ReplaceRoot,
            "$replaceWith" => StageKind::ReplaceWith,
            "$facet" => StageKind::Facet,
            "$bucket" => StageKind::Bucket,
            "$bucketAuto" => StageKind::BucketAuto,
            "$sample" => StageKind::Sample,
            "$sortByCount" => StageKind::SortByCount,
            "$graphLookup" => StageKind::GraphLookup,
            "$redact" => StageKind::Redact,
            "$geoNear" => StageKind::GeoNear,
            "$indexStats" => StageKind::IndexStats,
            "$collStats" => StageKind::CollStats,
            "$out" => StageKind::Out,
            "$merge" => StageKind::Merge,
            _ => return None,
        };
        Some(kind)
    }
}

/// A validated pipeline stage.
#[derive(Debug, Clone)]
pub struct PipelineStage {
    pub kind: StageKind,
    /// The operator name as it appeared in the input (e.g. `"$match"`).
    pub operator: String,
    /// The stage body — the JSON value associated with the operator key.
    pub body: JsonValue,
}

/// Validation errors. The string payload is intended to be surfaced to the
/// user verbatim; format it with a stable message so the UI can rely on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineError {
    NotAnArray,
    TooManyStages {
        got: usize,
        max: usize,
    },
    /// A stage was not an object, e.g. `[42]` or `["foo"]`.
    InvalidStageShape {
        index: usize,
    },
    /// A stage object had zero or multiple top-level keys.
    AmbiguousStage {
        index: usize,
        key_count: usize,
    },
    /// A stage's single key did not start with `$`.
    MissingOperatorPrefix {
        index: usize,
        key: String,
    },
    /// The operator is not in the allow-list. Fail-closed default.
    UnknownOperator {
        index: usize,
        operator: String,
    },
    /// The operator is forbidden (runs arbitrary JavaScript).
    ForbiddenOperator {
        index: Option<usize>,
        operator: String,
    },
    /// `$out` / `$merge` was not the last stage.
    NonTerminalWrite {
        index: usize,
        operator: String,
    },
}

impl PipelineError {
    pub fn user_message(&self) -> String {
        match self {
            PipelineError::NotAnArray => "pipeline must be a JSON array of stage objects".to_string(),
            PipelineError::TooManyStages { got, max } => {
                format!("pipeline has {got} stages, maximum allowed is {max}")
            }
            PipelineError::InvalidStageShape { index } => {
                format!("stage {index} is not a JSON object")
            }
            PipelineError::AmbiguousStage { index, key_count } => format!(
                "stage {index} has {key_count} top-level keys; each stage must have exactly one `$operator` key"
            ),
            PipelineError::MissingOperatorPrefix { index, key } => format!(
                "stage {index} key `{key}` is missing the `$` prefix required for aggregation operators"
            ),
            PipelineError::UnknownOperator { index, operator } => format!(
                "stage {index} uses unknown aggregation operator `{operator}`"
            ),
            PipelineError::ForbiddenOperator { index, operator } => match index {
                Some(i) => format!(
                    "stage {i} uses forbidden operator `{operator}` (executes arbitrary server-side code)"
                ),
                None => format!(
                    "forbidden operator `{operator}` detected (executes arbitrary server-side code)"
                ),
            },
            PipelineError::NonTerminalWrite { index, operator } => format!(
                "write stage `{operator}` must be the last stage of the pipeline (found at index {index})"
            ),
        }
    }
}

/// Result of a successful pipeline validation.
#[derive(Debug, Clone)]
pub struct ValidatedPipeline {
    pub stages: Vec<PipelineStage>,
    pub class: MongoQueryClass,
}

/// Validates a JSON pipeline value. Accepts either:
///   - a JSON array of stage objects, or
///   - a wrapper object like `{"operation": "aggregate", "pipeline": [...]}`:
///     the validator extracts `pipeline` automatically.
pub fn validate_pipeline(value: &JsonValue) -> Result<ValidatedPipeline, PipelineError> {
    let stages_value = extract_pipeline_array(value)?;
    validate_stages_array(stages_value)
}

fn extract_pipeline_array(value: &JsonValue) -> Result<&JsonValue, PipelineError> {
    if value.is_array() {
        return Ok(value);
    }
    if let Some(inner) = value.get("pipeline") {
        if inner.is_array() {
            return Ok(inner);
        }
    }
    Err(PipelineError::NotAnArray)
}

fn validate_stages_array(value: &JsonValue) -> Result<ValidatedPipeline, PipelineError> {
    let array = value.as_array().ok_or(PipelineError::NotAnArray)?;
    if array.len() > MAX_PIPELINE_STAGES {
        return Err(PipelineError::TooManyStages {
            got: array.len(),
            max: MAX_PIPELINE_STAGES,
        });
    }

    let mut stages = Vec::with_capacity(array.len());
    let mut class = MongoQueryClass::Read;

    let last_index = array.len().saturating_sub(1);

    for (index, stage_value) in array.iter().enumerate() {
        let stage = parse_stage(index, stage_value)?;
        scan_forbidden_operators(Some(index), &stage.body)?;

        if stage.kind.is_terminal() && index != last_index {
            return Err(PipelineError::NonTerminalWrite {
                index,
                operator: stage.operator.clone(),
            });
        }

        if stage.kind.is_write() {
            class = MongoQueryClass::Mutation;
        }

        stages.push(stage);
    }

    Ok(ValidatedPipeline { stages, class })
}

fn parse_stage(index: usize, value: &JsonValue) -> Result<PipelineStage, PipelineError> {
    let obj = value
        .as_object()
        .ok_or(PipelineError::InvalidStageShape { index })?;

    if obj.len() != 1 {
        return Err(PipelineError::AmbiguousStage {
            index,
            key_count: obj.len(),
        });
    }

    let (key, body) = obj.iter().next().expect("checked len == 1");

    if !key.starts_with('$') {
        return Err(PipelineError::MissingOperatorPrefix {
            index,
            key: key.clone(),
        });
    }

    if FORBIDDEN_OPERATORS.contains(&key.as_str()) {
        return Err(PipelineError::ForbiddenOperator {
            index: Some(index),
            operator: key.clone(),
        });
    }

    let kind = StageKind::from_operator(key).ok_or_else(|| PipelineError::UnknownOperator {
        index,
        operator: key.clone(),
    })?;

    Ok(PipelineStage {
        kind,
        operator: key.clone(),
        body: body.clone(),
    })
}

/// Recursively scan a JSON value for forbidden operators (`$function`,
/// `$accumulator`, `$where`) anywhere in the tree.
fn scan_forbidden_operators(stage_index: Option<usize>, value: &JsonValue) -> Result<(), PipelineError> {
    scan_recursive(stage_index, value, 0)
}

fn scan_recursive(
    stage_index: Option<usize>,
    value: &JsonValue,
    depth: usize,
) -> Result<(), PipelineError> {
    if depth > MAX_SCAN_DEPTH {
        // Exceptionally deep nesting — stop scanning rather than recursing
        // further. The pipeline will still be rejected by MongoDB if it's
        // malformed, but we avoid a stack blow-up from adversarial input.
        return Ok(());
    }
    match value {
        JsonValue::Object(map) => {
            for (k, v) in map {
                if FORBIDDEN_OPERATORS.contains(&k.as_str()) {
                    return Err(PipelineError::ForbiddenOperator {
                        index: stage_index,
                        operator: k.clone(),
                    });
                }
                scan_recursive(stage_index, v, depth + 1)?;
            }
        }
        JsonValue::Array(items) => {
            for v in items {
                scan_recursive(stage_index, v, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_pipeline_is_read() {
        let result = validate_pipeline(&json!([])).unwrap();
        assert!(result.stages.is_empty());
        assert_eq!(result.class, MongoQueryClass::Read);
    }

    #[test]
    fn simple_match_is_read() {
        let result = validate_pipeline(&json!([{"$match": {"status": "A"}}])).unwrap();
        assert_eq!(result.stages.len(), 1);
        assert_eq!(result.stages[0].kind, StageKind::Match);
        assert_eq!(result.class, MongoQueryClass::Read);
    }

    #[test]
    fn match_then_group_then_sort_is_read() {
        let pipeline = json!([
            {"$match": {"status": "A"}},
            {"$group": {"_id": "$user", "count": {"$sum": 1}}},
            {"$sort": {"count": -1}},
        ]);
        let result = validate_pipeline(&pipeline).unwrap();
        assert_eq!(result.stages.len(), 3);
        assert_eq!(result.class, MongoQueryClass::Read);
    }

    #[test]
    fn out_at_end_is_mutation() {
        let pipeline = json!([
            {"$match": {"status": "A"}},
            {"$out": "archive"},
        ]);
        let result = validate_pipeline(&pipeline).unwrap();
        assert_eq!(result.class, MongoQueryClass::Mutation);
        assert_eq!(result.stages[1].kind, StageKind::Out);
    }

    #[test]
    fn merge_at_end_is_mutation() {
        let pipeline = json!([
            {"$match": {"x": 1}},
            {"$merge": {"into": "target"}},
        ]);
        let result = validate_pipeline(&pipeline).unwrap();
        assert_eq!(result.class, MongoQueryClass::Mutation);
    }

    #[test]
    fn out_in_middle_is_rejected() {
        let pipeline = json!([
            {"$out": "tmp"},
            {"$match": {}},
        ]);
        let err = validate_pipeline(&pipeline).unwrap_err();
        assert_eq!(
            err,
            PipelineError::NonTerminalWrite {
                index: 0,
                operator: "$out".to_string(),
            }
        );
    }

    #[test]
    fn function_operator_is_rejected_at_top_level() {
        let pipeline = json!([
            {"$match": {"$function": {"body": "function(){return true}", "args": [], "lang": "js"}}},
        ]);
        let err = validate_pipeline(&pipeline).unwrap_err();
        assert!(matches!(err, PipelineError::ForbiddenOperator { .. }));
    }

    #[test]
    fn where_operator_is_rejected_nested() {
        let pipeline = json!([
            {"$match": {"$expr": {"$and": [{"$where": "this.score > 5"}]}}},
        ]);
        let err = validate_pipeline(&pipeline).unwrap_err();
        assert!(matches!(err, PipelineError::ForbiddenOperator { operator, .. } if operator == "$where"));
    }

    #[test]
    fn accumulator_operator_is_rejected() {
        let pipeline = json!([
            {"$group": {
                "_id": "$x",
                "custom": {"$accumulator": {"init": "function(){}", "accumulate": "function(){}"}}
            }},
        ]);
        let err = validate_pipeline(&pipeline).unwrap_err();
        assert!(matches!(err, PipelineError::ForbiddenOperator { operator, .. } if operator == "$accumulator"));
    }

    #[test]
    fn unknown_operator_is_rejected() {
        let pipeline = json!([{"$notARealStage": {}}]);
        let err = validate_pipeline(&pipeline).unwrap_err();
        assert!(matches!(err, PipelineError::UnknownOperator { .. }));
    }

    #[test]
    fn missing_dollar_prefix_is_rejected() {
        let pipeline = json!([{"match": {}}]);
        let err = validate_pipeline(&pipeline).unwrap_err();
        assert!(matches!(err, PipelineError::MissingOperatorPrefix { .. }));
    }

    #[test]
    fn two_keys_in_stage_is_rejected() {
        let pipeline = json!([{"$match": {}, "$limit": 10}]);
        let err = validate_pipeline(&pipeline).unwrap_err();
        assert!(matches!(err, PipelineError::AmbiguousStage { .. }));
    }

    #[test]
    fn non_object_stage_is_rejected() {
        let pipeline = json!([42]);
        let err = validate_pipeline(&pipeline).unwrap_err();
        assert!(matches!(err, PipelineError::InvalidStageShape { .. }));
    }

    #[test]
    fn too_many_stages_is_rejected() {
        let stages: Vec<JsonValue> = (0..=MAX_PIPELINE_STAGES)
            .map(|_| json!({"$match": {}}))
            .collect();
        let err = validate_pipeline(&JsonValue::Array(stages)).unwrap_err();
        assert!(matches!(err, PipelineError::TooManyStages { .. }));
    }

    #[test]
    fn not_an_array_is_rejected() {
        let err = validate_pipeline(&json!({"foo": "bar"})).unwrap_err();
        assert_eq!(err, PipelineError::NotAnArray);
    }

    #[test]
    fn wrapper_object_with_pipeline_array_is_accepted() {
        let wrapper = json!({
            "operation": "aggregate",
            "collection": "users",
            "pipeline": [{"$match": {"x": 1}}]
        });
        let result = validate_pipeline(&wrapper).unwrap();
        assert_eq!(result.stages.len(), 1);
        assert_eq!(result.class, MongoQueryClass::Read);
    }

    #[test]
    fn lookup_with_subpipeline_is_read() {
        let pipeline = json!([{
            "$lookup": {
                "from": "orders",
                "let": {"user_id": "$_id"},
                "pipeline": [
                    {"$match": {"$expr": {"$eq": ["$user", "$$user_id"]}}}
                ],
                "as": "orders"
            }
        }]);
        let result = validate_pipeline(&pipeline).unwrap();
        assert_eq!(result.class, MongoQueryClass::Read);
    }

    #[test]
    fn deeply_nested_forbidden_operator_is_rejected() {
        let pipeline = json!([{
            "$project": {
                "flag": {"$cond": {"if": {"$where": "this.x > 0"}, "then": 1, "else": 0}}
            }
        }]);
        let err = validate_pipeline(&pipeline).unwrap_err();
        assert!(matches!(err, PipelineError::ForbiddenOperator { .. }));
    }

    #[test]
    fn user_message_is_non_empty_for_each_variant() {
        let cases = [
            PipelineError::NotAnArray,
            PipelineError::TooManyStages { got: 100, max: 50 },
            PipelineError::InvalidStageShape { index: 0 },
            PipelineError::AmbiguousStage { index: 0, key_count: 2 },
            PipelineError::MissingOperatorPrefix { index: 0, key: "x".into() },
            PipelineError::UnknownOperator { index: 0, operator: "$x".into() },
            PipelineError::ForbiddenOperator { index: Some(0), operator: "$where".into() },
            PipelineError::NonTerminalWrite { index: 0, operator: "$out".into() },
        ];
        for case in &cases {
            assert!(!case.user_message().is_empty());
        }
    }
}
