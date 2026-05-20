// SPDX-License-Identifier: Apache-2.0

//! Custom SQLite scalar functions registered on every connection.
//!
//! ## `if(cond, then, else)`
//!
//! SQLite has no native `if()` function — it ships `iif()` and `CASE WHEN`
//! instead. Databases created or migrated with MySQL-compatible tooling often
//! contain views whose body calls `if()`: SQLite accepts such a view at
//! `CREATE VIEW` time but only fails at query time with `no such function: if`.
//! Registering an `if()` alias with `iif()` semantics lets those views load.

use std::os::raw::c_int;

use libsqlite3_sys as ffi;

/// SQLite requires the function name as a NUL-terminated C string.
static IF_FN_NAME: &[u8] = b"if\0";

/// Registers QoreDB's custom `if()` scalar function on a raw SQLite handle.
pub fn register(db: *mut ffi::sqlite3) -> Result<(), String> {
    // `if` always takes exactly three arguments, mirroring MySQL's
    // `IF(expr, true_value, false_value)`.
    let rc = unsafe {
        ffi::sqlite3_create_function_v2(
            db,
            IF_FN_NAME.as_ptr().cast(),
            3,
            ffi::SQLITE_UTF8 | ffi::SQLITE_DETERMINISTIC,
            std::ptr::null_mut(),
            Some(if_func),
            None,
            None,
            None,
        )
    };

    if rc == ffi::SQLITE_OK {
        Ok(())
    } else {
        Err(format!("sqlite3_create_function_v2(if) returned {rc}"))
    }
}

/// `if(cond, then_value, else_value)` — returns `then_value` when `cond` is
/// true and `else_value` otherwise, following SQLite's truth rules.
unsafe extern "C" fn if_func(
    ctx: *mut ffi::sqlite3_context,
    n_arg: c_int,
    args: *mut *mut ffi::sqlite3_value,
) {
    if n_arg != 3 {
        ffi::sqlite3_result_error_code(ctx, ffi::SQLITE_MISUSE);
        return;
    }

    let branch = if is_truthy(*args.offset(0)) {
        *args.offset(1)
    } else {
        *args.offset(2)
    };

    ffi::sqlite3_result_value(ctx, branch);
}

/// Evaluates a value's truthiness the way SQLite's `CASE WHEN` / `iif()` do:
/// NULL and numeric zero are false; text is coerced to a number first.
unsafe fn is_truthy(value: *mut ffi::sqlite3_value) -> bool {
    match ffi::sqlite3_value_type(value) {
        ffi::SQLITE_NULL => false,
        ffi::SQLITE_INTEGER => ffi::sqlite3_value_int64(value) != 0,
        // REAL, TEXT and BLOB go through numeric coercion: text such as "0"
        // or "abc" becomes 0.0 (false) while "1" becomes 1.0 (true).
        _ => ffi::sqlite3_value_double(value) != 0.0,
    }
}
