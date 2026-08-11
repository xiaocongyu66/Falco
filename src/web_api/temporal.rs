//! Temporal API — modern date/time handling (TC39 proposal).
//!
//! # Overview
//!
//! The Temporal API is the modern replacement for the legacy `Date` object.
//! It provides:
//!
//! - **Temporal.Now** — current instant, timezone, plain date/time
//! - **Temporal.Instant** — a moment in time (nanosecond precision)
//! - **Temporal.PlainDate** — a calendar date (year, month, day) without time
//! - **Temporal.PlainTime** — a time of day (hour, minute, second, ns) without date
//! - **Temporal.PlainDateTime** — combination of date and time
//! - **Temporal.ZonedDateTime** — date/time with timezone
//! - **Temporal.Duration** — a length of time (years, months, days, hours, etc.)
//! - **Temporal.PlainYearMonth** — a year and month (no day)
//! - **Temporal.PlainMonthDay** — a month and day (no year)
//!
//! All types are immutable and support arithmetic, comparison, and
//! ISO 8601 string formatting/parsing.
//!
//! # Precision
//!
//! Temporal uses nanosecond precision. Internally, instants are stored as
//! `i128` nanoseconds since the Unix epoch (1970-01-01T00:00:00Z).

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Nanoseconds per second.
const NS_PER_SEC: i128 = 1_000_000_000;
/// Nanoseconds per day.
const NS_PER_DAY: i128 = 86_400 * NS_PER_SEC;
/// Nanoseconds per millisecond.
const NS_PER_MS: i128 = 1_000_000;

/// Register the Temporal API.
pub fn register(scope: &mut Scope) {
    let mut temporal_obj = ObjectValue::new();

    // Temporal.Now
    register_now(&mut temporal_obj);

    // Temporal.Instant
    register_instant(&mut temporal_obj);

    // Temporal.PlainDate
    register_plain_date(&mut temporal_obj);

    // Temporal.PlainTime
    register_plain_time(&mut temporal_obj);

    // Temporal.PlainDateTime
    register_plain_date_time(&mut temporal_obj);

    // Temporal.Duration
    register_duration(&mut temporal_obj);

    // Temporal.PlainYearMonth
    register_plain_year_month(&mut temporal_obj);

    // Temporal.PlainMonthDay
    register_plain_month_day(&mut temporal_obj);

    // Temporal.ZonedDateTime (simplified — uses UTC only)
    register_zoned_date_time(&mut temporal_obj);

    scope.declare("Temporal", Value::Object(Rc::new(RefCell::new(temporal_obj))));
}

// ── Temporal.Now ──────────────────────────────────────────────────────

fn register_now(temporal: &mut ObjectValue) {
    let mut now_obj = ObjectValue::new();

    now_obj.set(
        "instant",
        Value::Builtin(BuiltinFn {
            name: "Temporal.Now.instant".to_string(),
            func: Rc::new(|_args| {
                let ns = current_ns_since_epoch();
                make_instant(ns)
            }),
        }),
    );

    now_obj.set(
        "plainDateTimeISO",
        Value::Builtin(BuiltinFn {
            name: "Temporal.Now.plainDateTimeISO".to_string(),
            func: Rc::new(|_args| {
                let ns = current_ns_since_epoch();
                let (y, mo, d, h, mi, s, ms) = ns_to_components(ns);
                make_plain_date_time(y, mo, d, h, mi, s, ms)
            }),
        }),
    );

    now_obj.set(
        "plainDateISO",
        Value::Builtin(BuiltinFn {
            name: "Temporal.Now.plainDateISO".to_string(),
            func: Rc::new(|_args| {
                let ns = current_ns_since_epoch();
                let (y, mo, d, _, _, _, _) = ns_to_components(ns);
                make_plain_date(y, mo, d)
            }),
        }),
    );

    now_obj.set(
        "plainTimeISO",
        Value::Builtin(BuiltinFn {
            name: "Temporal.Now.plainTimeISO".to_string(),
            func: Rc::new(|_args| {
                let ns = current_ns_since_epoch();
                let (_, _, _, h, mi, s, ms) = ns_to_components(ns);
                make_plain_time(h, mi, s, ms)
            }),
        }),
    );

    now_obj.set(
        "zonedDateTimeISO",
        Value::Builtin(BuiltinFn {
            name: "Temporal.Now.zonedDateTimeISO".to_string(),
            func: Rc::new(|_args| {
                let ns = current_ns_since_epoch();
                make_zoned_date_time(ns, "UTC".to_string())
            }),
        }),
    );

    now_obj.set(
        "timeZoneId",
        Value::Builtin(BuiltinFn {
            name: "Temporal.Now.timeZoneId".to_string(),
            func: Rc::new(|_args| Ok(Value::String("UTC".to_string()))),
        }),
    );

    temporal.set("Now", Value::Object(Rc::new(RefCell::new(now_obj))));
}

// ── Temporal.Instant ──────────────────────────────────────────────────

fn register_instant(temporal: &mut ObjectValue) {
    temporal.set(
        "Instant",
        Value::Builtin(BuiltinFn {
            name: "Temporal.Instant".to_string(),
            func: Rc::new(|args| {
                let ns = if let Some(Value::String(s)) = args.first() {
                    parse_instant_string(s)?
                } else if let Some(Value::Number(n)) = args.first() {
                    (*n as i64 * 1_000_000) as i128
                } else {
                    current_ns_since_epoch()
                };
                make_instant(ns)
            }),
        }),
    );
}

fn make_instant(ns: i128) -> Result<Value, String> {
    let mut obj = ObjectValue::new();
    obj.set("epochNanoseconds", Value::Number(ns as f64));
    obj.set("epochMilliseconds", Value::Number((ns / NS_PER_MS) as f64));
    obj.set("epochSeconds", Value::Number((ns / NS_PER_SEC) as f64));

    let ns_for_fmt = ns;
    obj.set(
        "toString",
        Value::Builtin(BuiltinFn {
            name: "Instant.toString".to_string(),
            func: Rc::new(move |_args| {
                Ok(Value::String(format_instant(ns_for_fmt)))
            }),
        }),
    );

    let ns_for_toJSON = ns;
    obj.set(
        "toJSON",
        Value::Builtin(BuiltinFn {
            name: "Instant.toJSON".to_string(),
            func: Rc::new(move |_args| {
                Ok(Value::String(format_instant(ns_for_toJSON)))
            }),
        }),
    );

    let ns_for_epoch = ns;
    obj.set(
        "toEpochSeconds",
        Value::Builtin(BuiltinFn {
            name: "Instant.toEpochSeconds".to_string(),
            func: Rc::new(move |_args| {
                Ok(Value::Number((ns_for_epoch / NS_PER_SEC) as f64))
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(obj))))
}

/// Parse an ISO 8601 instant string (e.g., "2024-01-15T10:30:00Z").
fn parse_instant_string(s: &str) -> Result<i128, String> {
    // This is a simplified parser. Full ISO 8601 parsing is complex.
    // Format: YYYY-MM-DDTHH:MM:SS[.sss]Z
    if s.len() < 20 {
        return Err(format!("invalid instant string: {}", s));
    }
    let year: i32 = s[0..4].parse().map_err(|_| "invalid year")?;
    let month: u32 = s[5..7].parse().map_err(|_| "invalid month")?;
    let day: u32 = s[8..10].parse().map_err(|_| "invalid day")?;
    let hour: u32 = s[11..13].parse().map_err(|_| "invalid hour")?;
    let minute: u32 = s[14..16].parse().map_err(|_| "invalid minute")?;
    let second: u32 = s[17..19].parse().map_err(|_| "invalid second")?;
    let ms = if s.len() > 23 && s.as_bytes()[19] == b'.' {
        s[20..23].parse().unwrap_or(0)
    } else {
        0
    };

    let days = ymd_to_days(year, month, day);
    let ns = (days as i128) * NS_PER_DAY
        + (hour as i128) * 3600 * NS_PER_SEC
        + (minute as i128) * 60 * NS_PER_SEC
        + (second as i128) * NS_PER_SEC
        + (ms as i128) * NS_PER_MS;
    Ok(ns)
}

/// Format an instant as an ISO 8601 string.
fn format_instant(ns: i128) -> String {
    let (y, mo, d, h, mi, s, ms) = ns_to_components(ns);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        y, mo, d, h, mi, s, ms
    )
}

// ── Temporal.PlainDate ────────────────────────────────────────────────

fn register_plain_date(temporal: &mut ObjectValue) {
    temporal.set(
        "PlainDate",
        Value::Builtin(BuiltinFn {
            name: "Temporal.PlainDate".to_string(),
            func: Rc::new(|args| {
                let year = args.first().map(|v| v.to_number() as i32).unwrap_or(1970);
                let month = args.get(1).map(|v| v.to_number() as u32).unwrap_or(1);
                let day = args.get(2).map(|v| v.to_number() as u32).unwrap_or(1);
                make_plain_date(year, month, day)
            }),
        }),
    );
}

fn make_plain_date(year: i32, month: u32, day: u32) -> Result<Value, String> {
    let mut obj = ObjectValue::new();
    obj.set("year", Value::Number(year as f64));
    obj.set("month", Value::Number(month as f64));
    obj.set("day", Value::Number(day as f64));
    obj.set("monthCode", Value::String(format!("M{:02}", month)));
    obj.set("calendar", Value::String("iso8601".to_string()));

    let y_clone = year;
    let m_clone = month;
    let d_clone = day;
    obj.set(
        "toString",
        Value::Builtin(BuiltinFn {
            name: "PlainDate.toString".to_string(),
            func: Rc::new(move |_args| {
                Ok(Value::String(format!(
                    "{:04}-{:02}-{:02}",
                    y_clone, m_clone, d_clone
                )))
            }),
        }),
    );

    // dayOfWeek (1=Monday, 7=Sunday)
    let day_of_week = day_of_week(year, month, day);
    obj.set("dayOfWeek", Value::Number(day_of_week as f64));
    obj.set("dayOfYear", Value::Number(day_of_year(year, month, day) as f64));
    obj.set("daysInWeek", Value::Number(7.0));
    obj.set("daysInYear", Value::Number(days_in_year(year) as f64));
    obj.set("daysInMonth", Value::Number(days_in_month(year, month) as f64));
    obj.set("monthsInYear", Value::Number(12.0));
    obj.set("inLeapYear", Value::Boolean(is_leap_year(year)));

    // .add(duration)
    let y_add = year;
    let m_add = month;
    let d_add = day;
    obj.set(
        "add",
        Value::Builtin(BuiltinFn {
            name: "PlainDate.add".to_string(),
            func: Rc::new(move |args| {
                // Simplified: parse a duration-like object.
                let years = get_num(args.first(), "years");
                let months = get_num(args.first(), "months");
                let days = get_num(args.first(), "days");
                let (ny, nm, nd) = add_to_date(y_add, m_add, d_add, years, months, days);
                make_plain_date(ny, nm, nd)
            }),
        }),
    );

    // .until(other) — returns a Duration
    let y_until = year;
    let m_until = month;
    let d_until = day;
    obj.set(
        "until",
        Value::Builtin(BuiltinFn {
            name: "PlainDate.until".to_string(),
            func: Rc::new(move |args| {
                let other = args.first().cloned().unwrap_or(Value::Undefined);
                if let Value::Object(o) = &other {
                    let o = o.borrow();
                    let oy = o.properties.get("year").map(|v| v.to_number() as i32).unwrap_or(0);
                    let om = o.properties.get("month").map(|v| v.to_number() as u32).unwrap_or(0);
                    let od = o.properties.get("day").map(|v| v.to_number() as u32).unwrap_or(0);
                    let days_diff = (ymd_to_days(oy, om, od) - ymd_to_days(y_until, m_until, d_until)) as i64;
                    make_duration(0, 0, 0, 0, 0, 0, days_diff, 0)
                } else {
                    Err("until: expected a PlainDate".to_string())
                }
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(obj))))
}

// ── Temporal.PlainTime ────────────────────────────────────────────────

fn register_plain_time(temporal: &mut ObjectValue) {
    temporal.set(
        "PlainTime",
        Value::Builtin(BuiltinFn {
            name: "Temporal.PlainTime".to_string(),
            func: Rc::new(|args| {
                let hour = args.first().map(|v| v.to_number() as u32).unwrap_or(0);
                let minute = args.get(1).map(|v| v.to_number() as u32).unwrap_or(0);
                let second = args.get(2).map(|v| v.to_number() as u32).unwrap_or(0);
                let ms = args.get(3).map(|v| v.to_number() as u32).unwrap_or(0);
                make_plain_time(hour, minute, second, ms)
            }),
        }),
    );
}

fn make_plain_time(hour: u32, minute: u32, second: u32, ms: u32) -> Result<Value, String> {
    let mut obj = ObjectValue::new();
    obj.set("hour", Value::Number(hour as f64));
    obj.set("minute", Value::Number(minute as f64));
    obj.set("second", Value::Number(second as f64));
    obj.set("millisecond", Value::Number(ms as f64));
    obj.set("microsecond", Value::Number(0.0));
    obj.set("nanosecond", Value::Number(0.0));

    let h = hour;
    let m = minute;
    let s = second;
    let ms_v = ms;
    obj.set(
        "toString",
        Value::Builtin(BuiltinFn {
            name: "PlainTime.toString".to_string(),
            func: Rc::new(move |_args| {
                let str = if ms_v > 0 {
                    format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms_v)
                } else {
                    format!("{:02}:{:02}:{:02}", h, m, s)
                };
                Ok(Value::String(str))
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(obj))))
}

// ── Temporal.PlainDateTime ────────────────────────────────────────────

fn register_plain_date_time(temporal: &mut ObjectValue) {
    temporal.set(
        "PlainDateTime",
        Value::Builtin(BuiltinFn {
            name: "Temporal.PlainDateTime".to_string(),
            func: Rc::new(|args| {
                let y = args.first().map(|v| v.to_number() as i32).unwrap_or(1970);
                let mo = args.get(1).map(|v| v.to_number() as u32).unwrap_or(1);
                let d = args.get(2).map(|v| v.to_number() as u32).unwrap_or(1);
                let h = args.get(3).map(|v| v.to_number() as u32).unwrap_or(0);
                let mi = args.get(4).map(|v| v.to_number() as u32).unwrap_or(0);
                let s = args.get(5).map(|v| v.to_number() as u32).unwrap_or(0);
                let ms = args.get(6).map(|v| v.to_number() as u32).unwrap_or(0);
                make_plain_date_time(y, mo, d, h, mi, s, ms)
            }),
        }),
    );
}

fn make_plain_date_time(
    y: i32,
    mo: u32,
    d: u32,
    h: u32,
    mi: u32,
    s: u32,
    ms: u32,
) -> Result<Value, String> {
    let mut obj = ObjectValue::new();
    obj.set("year", Value::Number(y as f64));
    obj.set("month", Value::Number(mo as f64));
    obj.set("day", Value::Number(d as f64));
    obj.set("hour", Value::Number(h as f64));
    obj.set("minute", Value::Number(mi as f64));
    obj.set("second", Value::Number(s as f64));
    obj.set("millisecond", Value::Number(ms as f64));

    let yc = y;
    let moc = mo;
    let dc = d;
    let hc = h;
    let mic = mi;
    let sc = s;
    let msc = ms;
    obj.set(
        "toString",
        Value::Builtin(BuiltinFn {
            name: "PlainDateTime.toString".to_string(),
            func: Rc::new(move |_args| {
                Ok(Value::String(format!(
                    "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}",
                    yc, moc, dc, hc, mic, sc, msc
                )))
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(obj))))
}

// ── Temporal.Duration ─────────────────────────────────────────────────

fn register_duration(temporal: &mut ObjectValue) {
    temporal.set(
        "Duration",
        Value::Builtin(BuiltinFn {
            name: "Temporal.Duration".to_string(),
            func: Rc::new(|args| {
                let years = args.first().map(|v| v.to_number() as i64).unwrap_or(0);
                let months = args.get(1).map(|v| v.to_number() as i64).unwrap_or(0);
                let weeks = args.get(2).map(|v| v.to_number() as i64).unwrap_or(0);
                let days = args.get(3).map(|v| v.to_number() as i64).unwrap_or(0);
                let hours = args.get(4).map(|v| v.to_number() as i64).unwrap_or(0);
                let minutes = args.get(5).map(|v| v.to_number() as i64).unwrap_or(0);
                let seconds = args.get(6).map(|v| v.to_number() as i64).unwrap_or(0);
                let ms = args.get(7).map(|v| v.to_number() as i64).unwrap_or(0);
                make_duration(years, months, weeks, days, hours, minutes, seconds, ms)
            }),
        }),
    );
}

#[allow(clippy::too_many_arguments)]
fn make_duration(
    years: i64,
    months: i64,
    weeks: i64,
    days: i64,
    hours: i64,
    minutes: i64,
    seconds: i64,
    ms: i64,
) -> Result<Value, String> {
    let mut obj = ObjectValue::new();
    obj.set("years", Value::Number(years as f64));
    obj.set("months", Value::Number(months as f64));
    obj.set("weeks", Value::Number(weeks as f64));
    obj.set("days", Value::Number(days as f64));
    obj.set("hours", Value::Number(hours as f64));
    obj.set("minutes", Value::Number(minutes as f64));
    obj.set("seconds", Value::Number(seconds as f64));
    obj.set("milliseconds", Value::Number(ms as f64));
    obj.set("sign", Value::Number(if years + months + weeks + days + hours + minutes + seconds + ms < 0 { -1.0 } else if years + months + weeks + days + hours + minutes + seconds + ms > 0 { 1.0 } else { 0.0 }));
    obj.set("blank", Value::Boolean(years + months + weeks + days + hours + minutes + seconds + ms == 0));

    let yc = years;
    let moc = months;
    let wc = weeks;
    let dc = days;
    let hc = hours;
    let mic = minutes;
    let sc = seconds;
    let msc = ms;
    obj.set(
        "toString",
        Value::Builtin(BuiltinFn {
            name: "Duration.toString".to_string(),
            func: Rc::new(move |_args| {
                Ok(Value::String(format_duration(
                    yc, moc, wc, dc, hc, mic, sc, msc,
                )))
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(obj))))
}

/// Format a duration as an ISO 8601 duration string.
#[allow(clippy::too_many_arguments)]
fn format_duration(
    years: i64,
    months: i64,
    weeks: i64,
    days: i64,
    hours: i64,
    minutes: i64,
    seconds: i64,
    ms: i64,
) -> String {
    if years == 0 && months == 0 && weeks == 0 && days == 0 && hours == 0 && minutes == 0 && seconds == 0 && ms == 0 {
        return "PT0S".to_string();
    }
    let mut s = String::from("P");
    if years != 0 {
        s.push_str(&format!("{}Y", years.abs()));
    }
    if months != 0 {
        s.push_str(&format!("{}M", months.abs()));
    }
    if weeks != 0 {
        s.push_str(&format!("{}W", weeks.abs()));
    }
    if days != 0 {
        s.push_str(&format!("{}D", days.abs()));
    }
    if hours != 0 || minutes != 0 || seconds != 0 || ms != 0 {
        s.push('T');
        if hours != 0 {
            s.push_str(&format!("{}H", hours.abs()));
        }
        if minutes != 0 {
            s.push_str(&format!("{}M", minutes.abs()));
        }
        if seconds != 0 || ms != 0 {
            let total = seconds.abs() as f64 + (ms.abs() as f64 / 1000.0);
            s.push_str(&format!("{}S", total));
        }
    }
    if years + months + weeks + days + hours + minutes + seconds + ms < 0 {
        format!("-{}", s)
    } else {
        s
    }
}

// ── Temporal.PlainYearMonth ───────────────────────────────────────────

fn register_plain_year_month(temporal: &mut ObjectValue) {
    temporal.set(
        "PlainYearMonth",
        Value::Builtin(BuiltinFn {
            name: "Temporal.PlainYearMonth".to_string(),
            func: Rc::new(|args| {
                let year = args.first().map(|v| v.to_number() as i32).unwrap_or(1970);
                let month = args.get(1).map(|v| v.to_number() as u32).unwrap_or(1);
                let mut obj = ObjectValue::new();
                obj.set("year", Value::Number(year as f64));
                obj.set("month", Value::Number(month as f64));
                obj.set("daysInMonth", Value::Number(days_in_month(year, month) as f64));
                obj.set("daysInYear", Value::Number(days_in_year(year) as f64));
                obj.set("inLeapYear", Value::Boolean(is_leap_year(year)));
                Ok(Value::Object(Rc::new(RefCell::new(obj))))
            }),
        }),
    );
}

// ── Temporal.PlainMonthDay ────────────────────────────────────────────

fn register_plain_month_day(temporal: &mut ObjectValue) {
    temporal.set(
        "PlainMonthDay",
        Value::Builtin(BuiltinFn {
            name: "Temporal.PlainMonthDay".to_string(),
            func: Rc::new(|args| {
                let month = args.first().map(|v| v.to_number() as u32).unwrap_or(1);
                let day = args.get(1).map(|v| v.to_number() as u32).unwrap_or(1);
                let mut obj = ObjectValue::new();
                obj.set("month", Value::Number(month as f64));
                obj.set("day", Value::Number(day as f64));
                obj.set("monthCode", Value::String(format!("M{:02}", month)));
                Ok(Value::Object(Rc::new(RefCell::new(obj))))
            }),
        }),
    );
}

// ── Temporal.ZonedDateTime ────────────────────────────────────────────

fn register_zoned_date_time(temporal: &mut ObjectValue) {
    temporal.set(
        "ZonedDateTime",
        Value::Builtin(BuiltinFn {
            name: "Temporal.ZonedDateTime".to_string(),
            func: Rc::new(|args| {
                let ns = args
                    .first()
                    .map(|v| v.to_number() as i128 * NS_PER_MS)
                    .unwrap_or_else(current_ns_since_epoch);
                let tz = args
                    .get(2)
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "UTC".to_string());
                make_zoned_date_time(ns, tz)
            }),
        }),
    );
}

fn make_zoned_date_time(ns: i128, tz: String) -> Result<Value, String> {
    let (y, mo, d, h, mi, s, ms) = ns_to_components(ns);
    let mut obj = ObjectValue::new();
    obj.set("year", Value::Number(y as f64));
    obj.set("month", Value::Number(mo as f64));
    obj.set("day", Value::Number(d as f64));
    obj.set("hour", Value::Number(h as f64));
    obj.set("minute", Value::Number(mi as f64));
    obj.set("second", Value::Number(s as f64));
    obj.set("millisecond", Value::Number(ms as f64));
    obj.set("timeZoneId", Value::String(tz.clone()));
    obj.set("epochNanoseconds", Value::Number(ns as f64));

    let tz_c = tz;
    obj.set(
        "toString",
        Value::Builtin(BuiltinFn {
            name: "ZonedDateTime.toString".to_string(),
            func: Rc::new(move |_args| {
                Ok(Value::String(format!(
                    "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}[{}]",
                    y, mo, d, h, mi, s, ms, tz_c
                )))
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(obj))))
}

// ── Date math helpers ─────────────────────────────────────────────────

/// Current time in nanoseconds since the Unix epoch.
fn current_ns_since_epoch() -> i128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i128,
        Err(_) => 0,
    }
}

/// Convert nanoseconds since epoch to (year, month, day, hour, minute, second, ms).
fn ns_to_components(ns: i128) -> (i32, u32, u32, u32, u32, u32, u32) {
    let days = (ns / NS_PER_DAY) as i64;
    let rem_ns = ns - (days as i128) * NS_PER_DAY;
    let rem_secs = rem_ns / NS_PER_SEC;
    let hour = (rem_secs / 3600) as u32;
    let minute = ((rem_secs % 3600) / 60) as u32;
    let second = (rem_secs % 60) as u32;
    let ms = ((rem_ns % NS_PER_SEC) / NS_PER_MS) as u32;
    let (y, m, d) = days_to_ymd(days);
    (y, m, d, hour, minute, second, ms)
}

/// Convert (year, month, day) to days since 1970-01-01.
fn ymd_to_days(year: i32, month: u32, day: u32) -> i64 {
    // Algorithm from Howard Hinnant's date library.
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as i64; // [0, 399]
    let m = month as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + (day as i64 - 1); // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    (era as i64) * 146097 + doe - 719468
}

/// Convert days since 1970-01-01 to (year, month, day).
fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year as i32, m as u32, d as u32)
}

/// Check if a year is a leap year.
fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Get the number of days in a year.
fn days_in_year(year: i32) -> u32 {
    if is_leap_year(year) {
        366
    } else {
        365
    }
}

/// Get the number of days in a month.
fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap_year(year) { 29 } else { 28 },
        _ => 30,
    }
}

/// Get the day of the week (1=Monday, 7=Sunday).
fn day_of_week(year: i32, month: u32, day: u32) -> u32 {
    let days = ymd_to_days(year, month, day);
    // 1970-01-01 was a Thursday (4th day of the week with Monday=1).
    let dow = ((days % 7) + 4) % 7;
    if dow <= 0 {
        (dow + 7) as u32
    } else {
        dow as u32
    }
}

/// Get the day of the year (1-366).
fn day_of_year(year: i32, month: u32, day: u32) -> u32 {
    let jan1 = ymd_to_days(year, 1, 1);
    let today = ymd_to_days(year, month, day);
    (today - jan1 + 1) as u32
}

/// Add years/months/days to a date.
fn add_to_date(year: i32, month: u32, day: u32, years: i64, months: i64, days: i64) -> (i32, u32, u32) {
    let total_months = (year * 12 + month as i32 - 1) as i64 + years * 12 + months;
    let new_year = (total_months / 12) as i32;
    let new_month = (total_months % 12 + 12) % 12 + 1;
    let total_days = ymd_to_days(new_year, new_month as u32, day) + days;
    let (ny, nm, nd) = days_to_ymd(total_days);
    (ny, nm, nd)
}

/// Helper: get a numeric property from a Value.
fn get_num(val: Option<&Value>, key: &str) -> i64 {
    if let Some(Value::Object(o)) = val {
        o.borrow().properties.get(key).map(|v| v.to_number() as i64).unwrap_or(0)
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporal_now_exists() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let temporal = scope.get("Temporal").unwrap();
        if let Value::Object(obj) = temporal {
            let obj = obj.borrow();
            assert!(obj.properties.contains_key("Now"));
        }
    }

    #[test]
    fn plain_date_creation() {
        let d = make_plain_date(2024, 1, 15).unwrap();
        if let Value::Object(obj) = d {
            let obj = obj.borrow();
            assert_eq!(obj.properties.get("year"), Some(&Value::Number(2024.0)));
            assert_eq!(obj.properties.get("month"), Some(&Value::Number(1.0)));
            assert_eq!(obj.properties.get("day"), Some(&Value::Number(15.0)));
        }
    }

    #[test]
    fn plain_date_to_string() {
        let d = make_plain_date(2024, 1, 15).unwrap();
        if let Value::Object(obj) = &d {
            let obj = obj.borrow();
            if let Some(Value::Builtin(tostring_fn)) = obj.properties.get("toString") {
                let result = (tostring_fn.func)(vec![]).unwrap();
                assert_eq!(result, Value::String("2024-01-15".to_string()));
            }
        }
    }

    #[test]
    fn leap_year_check() {
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(2023));
        assert!(is_leap_year(2000));
        assert!(!is_leap_year(1900));
    }

    #[test]
    fn days_in_month_check() {
        assert_eq!(days_in_month(2024, 2), 29); // leap year
        assert_eq!(days_in_month(2023, 2), 28);
        assert_eq!(days_in_month(2024, 1), 31);
        assert_eq!(days_in_month(2024, 4), 30);
    }

    #[test]
    fn day_of_week_check() {
        // 2024-01-15 is a Monday.
        assert_eq!(day_of_week(2024, 1, 15), 1);
        // 1970-01-01 was a Thursday.
        assert_eq!(day_of_week(1970, 1, 1), 4);
    }

    #[test]
    fn day_of_year_check() {
        assert_eq!(day_of_year(2024, 1, 1), 1);
        assert_eq!(day_of_year(2024, 12, 31), 366); // leap year
        assert_eq!(day_of_year(2023, 12, 31), 365);
    }

    #[test]
    fn ymd_days_round_trip() {
        let (y, m, d) = (2024, 6, 15);
        let days = ymd_to_days(y, m, d);
        let (ny, nm, nd) = days_to_ymd(days);
        assert_eq!((y, m, d), (ny, nm, nd));
    }

    #[test]
    fn epoch_round_trip() {
        // 1970-01-01 = day 0.
        let (y, m, d) = days_to_ymd(0);
        assert_eq!((y, m, d), (1970, 1, 1));
        assert_eq!(ymd_to_days(1970, 1, 1), 0);
    }

    #[test]
    fn duration_format() {
        let s = format_duration(1, 2, 0, 3, 4, 5, 6, 0);
        assert_eq!(s, "P1Y2M3DT4H5M6S");
    }

    #[test]
    fn duration_zero() {
        let s = format_duration(0, 0, 0, 0, 0, 0, 0, 0);
        assert_eq!(s, "PT0S");
    }

    #[test]
    fn duration_negative() {
        let s = format_duration(0, 0, 0, -1, 0, 0, 0, 0);
        assert_eq!(s, "-P1D");
    }

    #[test]
    fn instant_parse() {
        let ns = parse_instant_string("2024-01-15T10:30:00Z").unwrap();
        // 2024-01-15 is day 19737 (since 1970-01-01), plus 10:30:00 = 37800 seconds.
        let expected = 19737 * NS_PER_DAY + 37800 * NS_PER_SEC;
        assert_eq!(ns, expected);
    }

    #[test]
    fn instant_format() {
        let ns = parse_instant_string("2024-01-15T10:30:00Z").unwrap();
        let s = format_instant(ns);
        assert_eq!(s, "2024-01-15T10:30:00.000Z");
    }

    #[test]
    fn add_to_date_basic() {
        // 2024-01-15 + 1 year = 2025-01-15.
        let (y, m, d) = add_to_date(2024, 1, 15, 1, 0, 0);
        assert_eq!((y, m, d), (2025, 1, 15));
    }

    #[test]
    fn add_to_date_months() {
        // 2024-01-15 + 13 months = 2025-02-15.
        let (y, m, d) = add_to_date(2024, 1, 15, 0, 13, 0);
        assert_eq!((y, m, d), (2025, 2, 15));
    }

    #[test]
    fn plain_time_creation() {
        let t = make_plain_time(10, 30, 45, 0).unwrap();
        if let Value::Object(obj) = t {
            let obj = obj.borrow();
            assert_eq!(obj.properties.get("hour"), Some(&Value::Number(10.0)));
            assert_eq!(obj.properties.get("minute"), Some(&Value::Number(30.0)));
            assert_eq!(obj.properties.get("second"), Some(&Value::Number(45.0)));
        }
    }

    #[test]
    fn all_temporal_types_registered() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let temporal = scope.get("Temporal").unwrap();
        if let Value::Object(obj) = temporal {
            let obj = obj.borrow();
            for type_name in &[
                "Now", "Instant", "PlainDate", "PlainTime", "PlainDateTime",
                "Duration", "PlainYearMonth", "PlainMonthDay", "ZonedDateTime",
            ] {
                assert!(obj.properties.contains_key(*type_name), "missing: {}", type_name);
            }
        }
    }
}
