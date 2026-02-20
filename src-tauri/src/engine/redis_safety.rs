// SPDX-License-Identifier: Apache-2.0

//! Redis query safety classification.
//!
//! Used to determine mutation/destructive intent for read-only and
//! production safety policies.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedisQueryClass {
    Read,
    Mutation,
    Dangerous,
    Unknown,
}

pub fn classify(query: &str) -> RedisQueryClass {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return RedisQueryClass::Unknown;
    }

    let mut parts = trimmed.split_whitespace();
    let Some(command) = parts.next() else {
        return RedisQueryClass::Unknown;
    };

    let cmd = command.to_ascii_uppercase();
    let sub = parts.next().map(|s| s.to_ascii_uppercase());

    if is_dangerous_command(&cmd, sub.as_deref()) {
        return RedisQueryClass::Dangerous;
    }

    if is_read_command(&cmd, sub.as_deref()) {
        return RedisQueryClass::Read;
    }

    if is_mutation_command(&cmd, sub.as_deref()) {
        return RedisQueryClass::Mutation;
    }

    RedisQueryClass::Unknown
}

fn is_dangerous_command(cmd: &str, sub: Option<&str>) -> bool {
    match cmd {
        "FLUSHALL" | "FLUSHDB" | "SHUTDOWN" | "SWAPDB" => true,
        "CONFIG" => matches!(sub, Some("SET" | "REWRITE" | "RESETSTAT")),
        "SCRIPT" => matches!(sub, Some("FLUSH" | "KILL")),
        "FUNCTION" => matches!(sub, Some("FLUSH" | "DELETE" | "RESTORE")),
        "MODULE" => matches!(sub, Some("LOAD" | "LOADEX" | "UNLOAD")),
        "ACL" => matches!(sub, Some("SETUSER" | "DELUSER" | "LOAD" | "SAVE")),
        "CLUSTER" => matches!(sub, Some("RESET" | "FAILOVER")),
        _ => false,
    }
}

fn is_read_command(cmd: &str, sub: Option<&str>) -> bool {
    match cmd {
        "PING" | "ECHO" | "TIME" | "INFO" | "DBSIZE" | "TYPE" | "TTL" | "PTTL" | "EXISTS"
        | "SCAN" | "HSCAN" | "SSCAN" | "ZSCAN" | "KEYS" | "SELECT" | "AUTH" | "HELLO" => true,
        "GET" | "MGET" | "GETRANGE" | "STRLEN" => true,
        "HGET" | "HMGET" | "HGETALL" | "HKEYS" | "HVALS" | "HLEN" | "HEXISTS" => true,
        "LINDEX" | "LLEN" | "LRANGE" | "LPOS" => true,
        "SISMEMBER" | "SMISMEMBER" | "SMEMBERS" | "SCARD" | "SRANDMEMBER" => true,
        "ZCARD" | "ZRANGE" | "ZREVRANGE" | "ZRANK" | "ZREVRANK" | "ZSCORE" | "ZCOUNT" => true,
        "XRANGE" | "XREVRANGE" | "XLEN" | "XREAD" => true,
        "XINFO" => true,
        "CLIENT" => matches!(
            sub,
            Some(
                "ID"
                    | "INFO"
                    | "LIST"
                    | "GETNAME"
                    | "TRACKINGINFO"
                    | "NO-EVICT"
                    | "NO-TOUCH"
            )
        ),
        "COMMAND" => true,
        "FCALL_RO" => true,
        _ => false,
    }
}

fn is_mutation_command(cmd: &str, sub: Option<&str>) -> bool {
    match cmd {
        "SET" | "SETEX" | "PSETEX" | "MSET" | "MSETNX" | "APPEND" | "GETSET" | "SETRANGE"
        | "INCR" | "INCRBY" | "INCRBYFLOAT" | "DECR" | "DECRBY" => true,
        "DEL" | "UNLINK" | "EXPIRE" | "PEXPIRE" | "EXPIREAT" | "PEXPIREAT" | "PERSIST"
        | "RENAME" | "RENAMENX" | "MOVE" | "COPY" => true,
        "HSET" | "HMSET" | "HSETNX" | "HDEL" | "HINCRBY" | "HINCRBYFLOAT" => true,
        "LPUSH" | "RPUSH" | "LPOP" | "RPOP" | "LSET" | "LTRIM" | "LINSERT" | "LMOVE"
        | "BLMOVE" | "BLPOP" | "BRPOP" | "RPOPLPUSH" | "BRPOPLPUSH" => true,
        "SADD" | "SREM" | "SPOP" | "SMOVE" => true,
        "ZADD" | "ZREM" | "ZINCRBY" | "ZPOPMIN" | "ZPOPMAX" | "ZREMRANGEBYRANK"
        | "ZREMRANGEBYSCORE" | "ZREMRANGEBYLEX" => true,
        "XADD" | "XDEL" | "XTRIM" | "XSETID" | "XCLAIM" | "XAUTOCLAIM" => true,
        "XGROUP" => matches!(
            sub,
            Some("CREATE" | "CREATECONSUMER" | "DELCONSUMER" | "DESTROY" | "SETID")
        ),
        "MULTI" | "EXEC" | "DISCARD" | "WATCH" | "UNWATCH" => true,
        "EVAL" | "EVALSHA" | "FCALL" | "PUBLISH" | "SPUBLISH" => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_read_commands() {
        assert_eq!(classify("GET mykey"), RedisQueryClass::Read);
        assert_eq!(classify("XRANGE mystream - + COUNT 10"), RedisQueryClass::Read);
        assert_eq!(classify("SELECT 2"), RedisQueryClass::Read);
    }

    #[test]
    fn classifies_mutation_commands() {
        assert_eq!(classify("SET mykey value"), RedisQueryClass::Mutation);
        assert_eq!(classify("HSET myhash field value"), RedisQueryClass::Mutation);
        assert_eq!(classify("XADD mystream * f v"), RedisQueryClass::Mutation);
    }

    #[test]
    fn classifies_dangerous_commands() {
        assert_eq!(classify("FLUSHALL"), RedisQueryClass::Dangerous);
        assert_eq!(classify("CONFIG SET appendonly no"), RedisQueryClass::Dangerous);
        assert_eq!(classify("SCRIPT FLUSH"), RedisQueryClass::Dangerous);
    }

    #[test]
    fn unknown_when_empty_or_unrecognized() {
        assert_eq!(classify(""), RedisQueryClass::Unknown);
        assert_eq!(classify("   "), RedisQueryClass::Unknown);
        assert_eq!(classify("MYMODULE.CUSTOM foo"), RedisQueryClass::Unknown);
    }
}
