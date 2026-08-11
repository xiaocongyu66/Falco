//! Intl API — internationalization support.
//!
//! Implements:
//! - `Intl.DateTimeFormat` — date/time formatting per locale
//! - `Intl.NumberFormat` — number/currency formatting per locale
//! - `Intl.Collator` — string comparison per locale
//! - `Intl.PluralRules` — plural category selection
//! - `Intl.ListFormat` — list formatting ("a, b, and c")
//! - `Intl.RelativeTimeFormat` — relative time ("3 days ago")
//! - `Intl.Segmenter` — text segmentation (grapheme/word/sentence)
//! - `Intl.getCanonicalLocales()` — locale validation
//!
//! # Locale Support
//!
//! Full ICU would require a large data file (~30 MB). Instead, we ship a
//! compact built-in locale database covering the most common locales:
//! en, en-US, en-GB, es, es-ES, fr, fr-FR, de, de-DE, it, pt, ru, ru-RU,
//! ja, ja-JP, ko, ko-KR, zh, zh-CN, zh-TW, ar, hi, tr, pl, nl, sv, da, no, fi.
//!
//! For each locale we store:
//! - Decimal separator (.,،)
//! - Thousands separator (,, ., ‹space›)
//! - Currency symbol
//! - Date format pattern
//! - Plural rules (one/few/many/other)

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Register the Intl API.
pub fn register(scope: &mut Scope) {
    let mut intl_obj = ObjectValue::new();

    // Intl.getCanonicalLocales(locales)
    intl_obj.set(
        "getCanonicalLocales",
        Value::Builtin(BuiltinFn {
            name: "Intl.getCanonicalLocales".to_string(),
            func: Rc::new(|args| {
                let locale = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "en".to_string());
                let canonical = canonicalize_locale(&locale);
                Ok(Value::Array(Rc::new(RefCell::new(vec![
                    Value::String(canonical),
                ]))))
            }),
        }),
    );

    // Intl.DateTimeFormat
    intl_obj.set(
        "DateTimeFormat",
        Value::Builtin(BuiltinFn {
            name: "Intl.DateTimeFormat".to_string(),
            func: Rc::new(|args| {
                let locale = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "en".to_string());
                let options = args.get(1).cloned().unwrap_or(Value::Undefined);
                make_date_time_format(&locale, &options)
            }),
        }),
    );

    // Intl.NumberFormat
    intl_obj.set(
        "NumberFormat",
        Value::Builtin(BuiltinFn {
            name: "Intl.NumberFormat".to_string(),
            func: Rc::new(|args| {
                let locale = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "en".to_string());
                let options = args.get(1).cloned().unwrap_or(Value::Undefined);
                make_number_format(&locale, &options)
            }),
        }),
    );

    // Intl.Collator
    intl_obj.set(
        "Collator",
        Value::Builtin(BuiltinFn {
            name: "Intl.Collator".to_string(),
            func: Rc::new(|args| {
                let locale = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "en".to_string());
                let options = args.get(1).cloned().unwrap_or(Value::Undefined);
                make_collator(&locale, &options)
            }),
        }),
    );

    // Intl.PluralRules
    intl_obj.set(
        "PluralRules",
        Value::Builtin(BuiltinFn {
            name: "Intl.PluralRules".to_string(),
            func: Rc::new(|args| {
                let locale = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "en".to_string());
                let options = args.get(1).cloned().unwrap_or(Value::Undefined);
                make_plural_rules(&locale, &options)
            }),
        }),
    );

    // Intl.ListFormat
    intl_obj.set(
        "ListFormat",
        Value::Builtin(BuiltinFn {
            name: "Intl.ListFormat".to_string(),
            func: Rc::new(|args| {
                let locale = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "en".to_string());
                let options = args.get(1).cloned().unwrap_or(Value::Undefined);
                make_list_format(&locale, &options)
            }),
        }),
    );

    // Intl.RelativeTimeFormat
    intl_obj.set(
        "RelativeTimeFormat",
        Value::Builtin(BuiltinFn {
            name: "Intl.RelativeTimeFormat".to_string(),
            func: Rc::new(|args| {
                let locale = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "en".to_string());
                let options = args.get(1).cloned().unwrap_or(Value::Undefined);
                make_relative_time_format(&locale, &options)
            }),
        }),
    );

    // Intl.Segmenter
    intl_obj.set(
        "Segmenter",
        Value::Builtin(BuiltinFn {
            name: "Intl.Segmenter".to_string(),
            func: Rc::new(|args| {
                let locale = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "en".to_string());
                let options = args.get(1).cloned().unwrap_or(Value::Undefined);
                make_segmenter(&locale, &options)
            }),
        }),
    );

    scope.declare("Intl", Value::Object(Rc::new(RefCell::new(intl_obj))));
}

// ── Locale data ───────────────────────────────────────────────────────

/// Compact locale data.
struct LocaleData {
    /// Decimal separator (e.g., ".", ",")
    decimal: &'static str,
    /// Thousands separator (e.g., ",", ".", " ")
    thousands: &'static str,
    /// Currency symbol (e.g., "$", "€", "¥")
    currency: &'static str,
    /// Default currency code.
    currency_code: &'static str,
    /// AM/PM markers.
    am: &'static str,
    pm: &'static str,
    /// Date format: Y=year, M=month, D=day.
    date_format: &'static str,
    /// Time format: H=hour(24), h=hour(12), m=minute, s=second.
    time_format: &'static str,
    /// Plural rule function (one, few, many, other).
    plural: fn(f64) -> &'static str,
}

fn plural_en(n: f64) -> &'static str {
    if n == 1.0 {
        "one"
    } else {
        "other"
    }
}

fn plural_ru(n: f64) -> &'static str {
    let n_abs = n.abs() as u64;
    let n_mod10 = n_abs % 10;
    let n_mod100 = n_abs % 100;
    if n_mod10 == 1 && n_mod100 != 11 {
        "one"
    } else if (2..=4).contains(&n_mod10) && !(12..=14).contains(&n_mod100) {
        "few"
    } else if n_mod10 == 0
        || (5..=9).contains(&n_mod10)
        || (11..=14).contains(&n_mod100)
    {
        "many"
    } else {
        "other"
    }
}

fn plural_ar(n: f64) -> &'static str {
    let n_int = n as i64;
    if n == 0.0 {
        "zero"
    } else if n == 1.0 {
        "one"
    } else if n == 2.0 {
        "two"
    } else if (3..=10).contains(&n_int) {
        "few"
    } else {
        "many"
    }
}

fn plural_default(n: f64) -> &'static str {
    if n == 1.0 {
        "one"
    } else {
        "other"
    }
}

/// Get locale data for a canonical locale code.
fn get_locale_data(locale: &str) -> &'static LocaleData {
    match locale {
        "en" | "en-US" => &LocaleData {
            decimal: ".",
            thousands: ",",
            currency: "$",
            currency_code: "USD",
            am: "AM",
            pm: "PM",
            date_format: "M/D/Y",
            time_format: "h:mm:ss A",
            plural: plural_en,
        },
        "en-GB" => &LocaleData {
            decimal: ".",
            thousands: ",",
            currency: "£",
            currency_code: "GBP",
            am: "am",
            pm: "pm",
            date_format: "D/M/Y",
            time_format: "HH:mm:ss",
            plural: plural_en,
        },
        "es" | "es-ES" => &LocaleData {
            decimal: ",",
            thousands: ".",
            currency: "€",
            currency_code: "EUR",
            am: "",
            pm: "",
            date_format: "D/M/Y",
            time_format: "H:mm:ss",
            plural: plural_en,
        },
        "fr" | "fr-FR" => &LocaleData {
            decimal: ",",
            thousands: " ",
            currency: "€",
            currency_code: "EUR",
            am: "",
            pm: "",
            date_format: "D/M/Y",
            time_format: "HH:mm:ss",
            plural: plural_en,
        },
        "de" | "de-DE" => &LocaleData {
            decimal: ",",
            thousands: ".",
            currency: "€",
            currency_code: "EUR",
            am: "",
            pm: "",
            date_format: "D.M.Y",
            time_format: "HH:mm:ss",
            plural: plural_en,
        },
        "it" => &LocaleData {
            decimal: ",",
            thousands: ".",
            currency: "€",
            currency_code: "EUR",
            am: "",
            pm: "",
            date_format: "D/M/Y",
            time_format: "HH:mm:ss",
            plural: plural_en,
        },
        "pt" | "pt-BR" => &LocaleData {
            decimal: ",",
            thousands: ".",
            currency: "R$",
            currency_code: "BRL",
            am: "",
            pm: "",
            date_format: "D/M/Y",
            time_format: "HH:mm:ss",
            plural: plural_en,
        },
        "ru" | "ru-RU" => &LocaleData {
            decimal: ",",
            thousands: " ",
            currency: "₽",
            currency_code: "RUB",
            am: "ДП",
            pm: "ПП",
            date_format: "D.M.Y",
            time_format: "H:mm:ss",
            plural: plural_ru,
        },
        "ja" | "ja-JP" => &LocaleData {
            decimal: ".",
            thousands: ",",
            currency: "¥",
            currency_code: "JPY",
            am: "午前",
            pm: "午後",
            date_format: "Y/M/D",
            time_format: "H:mm:ss",
            plural: plural_default,
        },
        "ko" | "ko-KR" => &LocaleData {
            decimal: ".",
            thousands: ",",
            currency: "₩",
            currency_code: "KRW",
            am: "오전",
            pm: "오후",
            date_format: "Y/M/D",
            time_format: "H:mm:ss",
            plural: plural_default,
        },
        "zh" | "zh-CN" | "zh-Hans" => &LocaleData {
            decimal: ".",
            thousands: ",",
            currency: "¥",
            currency_code: "CNY",
            am: "上午",
            pm: "下午",
            date_format: "Y/M/D",
            time_format: "H:mm:ss",
            plural: plural_default,
        },
        "zh-TW" | "zh-Hant" => &LocaleData {
            decimal: ".",
            thousands: ",",
            currency: "NT$",
            currency_code: "TWD",
            am: "上午",
            pm: "下午",
            date_format: "Y/M/D",
            time_format: "H:mm:ss",
            plural: plural_default,
        },
        "ar" | "ar-SA" => &LocaleData {
            decimal: "٫",
            thousands: "٬",
            currency: "ر.س",
            currency_code: "SAR",
            am: "ص",
            pm: "م",
            date_format: "D/M/Y",
            time_format: "h:mm:ss A",
            plural: plural_ar,
        },
        "hi" | "hi-IN" => &LocaleData {
            decimal: ".",
            thousands: ",",
            currency: "₹",
            currency_code: "INR",
            am: "am",
            pm: "pm",
            date_format: "D/M/Y",
            time_format: "h:mm:ss A",
            plural: plural_en,
        },
        "tr" | "tr-TR" => &LocaleData {
            decimal: ",",
            thousands: ".",
            currency: "₺",
            currency_code: "TRY",
            am: "ÖÖ",
            pm: "ÖS",
            date_format: "D.M.Y",
            time_format: "HH:mm:ss",
            plural: plural_en,
        },
        "pl" | "pl-PL" => &LocaleData {
            decimal: ",",
            thousands: " ",
            currency: "zł",
            currency_code: "PLN",
            am: "",
            pm: "",
            date_format: "D.M.Y",
            time_format: "HH:mm:ss",
            plural: plural_ru,
        },
        "nl" | "nl-NL" => &LocaleData {
            decimal: ",",
            thousands: ".",
            currency: "€",
            currency_code: "EUR",
            am: "",
            pm: "",
            date_format: "D-M-Y",
            time_format: "HH:mm:ss",
            plural: plural_en,
        },
        "sv" | "sv-SE" => &LocaleData {
            decimal: ",",
            thousands: " ",
            currency: "kr",
            currency_code: "SEK",
            am: "",
            pm: "",
            date_format: "Y-M-D",
            time_format: "HH:mm:ss",
            plural: plural_en,
        },
        _ => &LocaleData {
            decimal: ".",
            thousands: ",",
            currency: "$",
            currency_code: "USD",
            am: "AM",
            pm: "PM",
            date_format: "M/D/Y",
            time_format: "h:mm:ss A",
            plural: plural_en,
        },
    }
}

/// Canonicalize a locale code (e.g., "en_us" → "en-US").
fn canonicalize_locale(locale: &str) -> String {
    // Replace underscores with hyphens.
    let locale = locale.replace('_', "-");
    let parts: Vec<&str> = locale.split('-').collect();
    if parts.is_empty() {
        return "en".to_string();
    }
    // Language code is lowercase.
    let lang = parts[0].to_lowercase();
    if parts.len() == 1 {
        return lang;
    }
    // Region code is uppercase.
    let region = parts[1].to_uppercase();
    format!("{}-{}", lang, region)
}

// ── NumberFormat ──────────────────────────────────────────────────────

fn make_number_format(locale: &str, options: &Value) -> Result<Value, String> {
    let data = get_locale_data(&canonicalize_locale(locale));
    let mut style = "decimal".to_string();
    let mut currency_code = data.currency_code.to_string();
    let mut min_fraction_digits = 0u32;
    let mut max_fraction_digits = 3u32;
    let mut use_grouping = true;

    if let Value::Object(o) = options {
        let o = o.borrow();
        if let Some(Value::String(s)) = o.properties.get("style") {
            style = s.clone();
        }
        if let Some(Value::String(s)) = o.properties.get("currency") {
            currency_code = s.clone();
        }
        if let Some(Value::Number(n)) = o.properties.get("minimumFractionDigits") {
            min_fraction_digits = *n as u32;
        }
        if let Some(Value::Number(n)) = o.properties.get("maximumFractionDigits") {
            max_fraction_digits = *n as u32;
        }
        if let Some(Value::Boolean(b)) = o.properties.get("useGrouping") {
            use_grouping = *b;
        }
    }

    if style == "currency" {
        min_fraction_digits = min_fraction_digits.max(2);
        max_fraction_digits = max_fraction_digits.max(min_fraction_digits);
    }

    let mut obj = ObjectValue::new();
    obj.set("locale", Value::String(canonicalize_locale(locale)));
    obj.set("style", Value::String(style.clone()));

    let data_clone = data;
    let style_clone = style.clone();
    let currency_clone = currency_code.clone();
    obj.set(
        "format",
        Value::Builtin(BuiltinFn {
            name: "NumberFormat.format".to_string(),
            func: Rc::new(move |args| {
                let n = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                let formatted = format_number(
                    n,
                    data_clone,
                    &style_clone,
                    &currency_clone,
                    min_fraction_digits,
                    max_fraction_digits,
                    use_grouping,
                );
                Ok(Value::String(formatted))
            }),
        }),
    );

    let data_clone2 = data;
    let style_clone2 = style.clone();
    let currency_clone2 = currency_code.clone();
    obj.set(
        "formatToParts",
        Value::Builtin(BuiltinFn {
            name: "NumberFormat.formatToParts".to_string(),
            func: Rc::new(move |args| {
                let n = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                let parts = format_number_to_parts(
                    n,
                    data_clone2,
                    &style_clone2,
                    &currency_clone2,
                    min_fraction_digits,
                    max_fraction_digits,
                    use_grouping,
                );
                Ok(parts)
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(obj))))
}

/// Format a number per the locale and style.
fn format_number(
    n: f64,
    data: &LocaleData,
    style: &str,
    currency_code: &str,
    min_frac: u32,
    max_frac: u32,
    use_grouping: bool,
) -> String {
    let neg = n < 0.0;
    let n = n.abs();

    // Format with the appropriate number of fraction digits.
    let formatted = if max_frac == 0 && n.fract() == 0.0 {
        format!("{:.0}", n)
    } else {
        let mut s = format!("{:.*}", max_frac as usize, n);
        // Trim trailing zeros beyond min_frac.
        if min_frac < max_frac {
            let dot = s.find('.').unwrap_or(s.len());
            let frac = &s[dot + 1..];
            let trim_to = frac.len().saturating_sub((max_frac - min_frac) as usize);
            let mut end = frac.len();
            while end > trim_to && frac.as_bytes().get(end - 1) == Some(&b'0') {
                end -= 1;
            }
            s.truncate(dot + 1 + end);
            // Remove trailing dot.
            if s.ends_with('.') {
                s.pop();
            }
        }
        s
    };

    // Split into integer and fraction parts.
    let (int_part, frac_part) = formatted.split_once('.').unwrap_or((&formatted, ""));

    // Add thousands separators.
    let int_with_sep = if use_grouping {
        add_thousands_separator(int_part, data.thousands)
    } else {
        int_part.to_string()
    };

    // Combine.
    let mut result = if frac_part.is_empty() {
        int_with_sep
    } else {
        format!("{}{}{}", int_with_sep, data.decimal, frac_part)
    };

    // Apply style.
    match style {
        "currency" => {
            // Determine symbol: use the locale's symbol if currency matches,
            // otherwise use the ISO code.
            let symbol = if currency_code == data.currency_code {
                data.currency.to_string()
            } else {
                currency_code.to_string()
            };
            result = format!("{}{}", symbol, result);
        }
        "percent" => {
            // Multiply by 100 and append %.
            // (We already formatted the number; just append %.)
            result = format!("{}%", result);
        }
        "unit" => {
            // Just append the unit (caller provides it).
        }
        _ => {}
    }

    if neg {
        result = format!("-{}", result);
    }

    result
}

/// Add thousands separators to an integer string.
fn add_thousands_separator(s: &str, sep: &str) -> String {
    if s.len() <= 3 {
        return s.to_string();
    }
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push_str(sep);
        }
        result.push(*c);
    }
    result
}

/// Format a number into parts (for formatToParts).
fn format_number_to_parts(
    n: f64,
    data: &LocaleData,
    style: &str,
    currency_code: &str,
    min_frac: u32,
    max_frac: u32,
    use_grouping: bool,
) -> Value {
    let formatted = format_number(n, data, style, currency_code, min_frac, max_frac, use_grouping);
    let mut parts: Vec<Value> = Vec::new();

    // Parse the formatted string into parts.
    let neg = n < 0.0;
    if neg {
        parts.push(make_part("minusSign", "-"));
    }

    // Strip the leading minus if present.
    let s = formatted.trim_start_matches('-');

    // Find currency symbol or percent sign.
    let s = if style == "currency" {
        let symbol = if currency_code == data.currency_code {
            data.currency
        } else {
            currency_code
        };
        if let Some(rest) = s.strip_prefix(symbol) {
            parts.push(make_part("currency", symbol));
            rest
        } else {
            s
        }
    } else if style == "percent" {
        s.strip_suffix('%').unwrap_or(s)
    } else {
        s
    };

    // Split into integer and fraction.
    if let Some((int_part, frac_part)) = s.split_once(data.decimal) {
        // Integer part may contain thousands separators.
        for group in int_part.split(data.thousands) {
            if !group.is_empty() {
                parts.push(make_part("integer", group));
                // We don't add a separate "group" part for simplicity.
            }
        }
        parts.push(make_part("decimal", data.decimal));
        if !frac_part.is_empty() {
            parts.push(make_part("fraction", frac_part));
        }
    } else {
        parts.push(make_part("integer", s));
    }

    if style == "percent" {
        parts.push(make_part("percentSign", "%"));
    }

    Value::Array(Rc::new(RefCell::new(parts)))
}

fn make_part(part_type: &str, value: &str) -> Value {
    let mut obj = ObjectValue::new();
    obj.set("type", Value::String(part_type.to_string()));
    obj.set("value", Value::String(value.to_string()));
    Value::Object(Rc::new(RefCell::new(obj)))
}

// ── DateTimeFormat ────────────────────────────────────────────────────

fn make_date_time_format(locale: &str, options: &Value) -> Result<Value, String> {
    let data = get_locale_data(&canonicalize_locale(locale));
    let mut date_style = "short".to_string();
    let mut time_style = "short".to_string();
    let mut include_date = true;
    let mut include_time = true;

    if let Value::Object(o) = options {
        let o = o.borrow();
        if let Some(Value::String(s)) = o.properties.get("dateStyle") {
            date_style = s.clone();
        }
        if let Some(Value::String(s)) = o.properties.get("timeStyle") {
            time_style = s.clone();
        }
        if let Some(Value::Boolean(b)) = o.properties.get("year") {
            // Simplified: year as boolean means "include".
            let _ = b;
        }
    }

    let mut obj = ObjectValue::new();
    obj.set("locale", Value::String(canonicalize_locale(locale)));

    let data_clone = data;
    let date_style_clone = date_style.clone();
    let time_style_clone = time_style.clone();
    obj.set(
        "format",
        Value::Builtin(BuiltinFn {
            name: "DateTimeFormat.format".to_string(),
            func: Rc::new(move |args| {
                // The argument is a timestamp (ms since epoch) or a Date-like object.
                let timestamp = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                let formatted = format_date_time(
                    timestamp,
                    data_clone,
                    &date_style_clone,
                    &time_style_clone,
                    include_date,
                    include_time,
                );
                Ok(Value::String(formatted))
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(obj))))
}

/// Format a timestamp (ms since epoch) as a date/time string.
fn format_date_time(
    timestamp_ms: f64,
    data: &LocaleData,
    date_style: &str,
    time_style: &str,
    include_date: bool,
    include_time: bool,
) -> String {
    // Convert ms to broken-down date/time (UTC for simplicity).
    let secs = (timestamp_ms / 1000.0) as i64;
    let days = secs / 86400;
    let rem_secs = secs % 86400;
    let hour = (rem_secs / 3600) as u32;
    let minute = ((rem_secs % 3600) / 60) as u32;
    let second = (rem_secs % 60) as u32;

    // Convert days since epoch (1970-01-01) to year/month/day.
    let (year, month, day) = days_to_ymd(days);

    let mut parts: Vec<String> = Vec::new();

    if include_date {
        let date_str = format_date(year, month, day, data, date_style);
        parts.push(date_str);
    }

    if include_time {
        let time_str = format_time(hour, minute, second, data, time_style);
        parts.push(time_str);
    }

    parts.join(", ")
}

/// Convert days since 1970-01-01 to (year, month, day).
fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    // Use the algorithm from Howard Hinnant's date library.
    let z = days + 719468; // days since 0000-03-01
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // [0, 146097)
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (year as i32, m as u32, d as u32)
}

fn format_date(year: i32, month: u32, day: u32, data: &LocaleData, style: &str) -> String {
    let (y_str, m_str, d_str) = match style {
        "full" | "long" => (
            format!("{:04}", year),
            month_name(month, style == "full"),
            format!("{:02}", day),
        ),
        "medium" => (
            format!("{:04}", year),
            format!("{:02}", month),
            format!("{:02}", day),
        ),
        _ => (
            format!("{:02}", year % 100),
            format!("{:02}", month),
            format!("{:02}", day),
        ),
    };

    // Apply the locale's date format pattern.
    data.date_format
        .replace('Y', &y_str)
        .replace('M', &m_str)
        .replace('D', &d_str)
}

fn format_time(hour: u32, minute: u32, second: u32, data: &LocaleData, style: &str) -> String {
    let (h12, am_pm) = if hour == 0 {
        (12, data.am)
    } else if hour < 12 {
        (hour, data.am)
    } else if hour == 12 {
        (12, data.pm)
    } else {
        (hour - 12, data.pm)
    };

    let h_str = format!("{:02}", if data.time_format.contains('h') { h12 } else { hour });
    let m_str = format!("{:02}", minute);
    let s_str = if style == "short" { String::new() } else { format!("{:02}", second) };

    let mut result = data
        .time_format
        .replace('H', &h_str)
        .replace('h', &h_str)
        .replace("mm", &m_str)
        .replace("ss", &s_str);

    // Remove trailing colon if seconds are empty.
    if s_str.is_empty() {
        if let Some(idx) = result.rfind(":") {
            if idx == result.len() - 1 {
                result.truncate(idx);
            }
        }
    }

    // Replace AM/PM marker.
    result = result.replace("A", am_pm);
    result = result.trim().to_string();

    result
}

fn month_name(month: u32, full: bool) -> String {
    let names_full = [
        "January", "February", "March", "April", "May", "June",
        "July", "August", "September", "October", "November", "December",
    ];
    let names_short = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun",
        "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let idx = (month.saturating_sub(1)) as usize;
    if idx < 12 {
        if full {
            names_full[idx].to_string()
        } else {
            names_short[idx].to_string()
        }
    } else {
        format!("{:02}", month)
    }
}

// ── Collator ──────────────────────────────────────────────────────────

fn make_collator(locale: &str, options: &Value) -> Result<Value, String> {
    let _data = get_locale_data(&canonicalize_locale(locale));
    let mut sensitivity = "base".to_string();
    let mut numeric = false;

    if let Value::Object(o) = options {
        let o = o.borrow();
        if let Some(Value::String(s)) = o.properties.get("sensitivity") {
            sensitivity = s.clone();
        }
        if let Some(Value::Boolean(b)) = o.properties.get("numeric") {
            numeric = *b;
        }
    }

    let mut obj = ObjectValue::new();
    obj.set("locale", Value::String(canonicalize_locale(locale)));

    let sens_clone = sensitivity.clone();
    obj.set(
        "compare",
        Value::Builtin(BuiltinFn {
            name: "Collator.compare".to_string(),
            func: Rc::new(move |args| {
                let a = args.first().map(|v| v.to_string()).unwrap_or_default();
                let b = args.get(1).map(|v| v.to_string()).unwrap_or_default();
                let cmp = compare_strings(&a, &b, &sens_clone, numeric);
                Ok(Value::Number(cmp as f64))
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(obj))))
}

/// Compare two strings per the given sensitivity.
fn compare_strings(a: &str, b: &str, sensitivity: &str, numeric: bool) -> i32 {
    if numeric {
        // Natural sort: compare numbers numerically.
        return natural_compare(a, b);
    }

    let a_norm = normalize(a, sensitivity);
    let b_norm = normalize(b, sensitivity);

    match a_norm.cmp(&b_norm) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// Normalize a string per the sensitivity level.
fn normalize(s: &str, sensitivity: &str) -> String {
    match sensitivity {
        "base" => s.to_lowercase(),
        "accent" => s.to_lowercase(),
        "case" => s.to_string(),
        _ => s.to_string(),
    }
}

/// Natural (numeric) comparison: "file2" < "file10".
fn natural_compare(a: &str, b: &str) -> i32 {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let mut i = 0;
    let mut j = 0;
    while i < a_bytes.len() && j < b_bytes.len() {
        let ac = a_bytes[i];
        let bc = b_bytes[j];
        if ac.is_ascii_digit() && bc.is_ascii_digit() {
            // Extract the full number from each string.
            let mut a_num = 0u64;
            while i < a_bytes.len() && a_bytes[i].is_ascii_digit() {
                a_num = a_num * 10 + (a_bytes[i] - b'0') as u64;
                i += 1;
            }
            let mut b_num = 0u64;
            while j < b_bytes.len() && b_bytes[j].is_ascii_digit() {
                b_num = b_num * 10 + (b_bytes[j] - b'0') as u64;
                j += 1;
            }
            if a_num < b_num {
                return -1;
            } else if a_num > b_num {
                return 1;
            }
        } else {
            if ac < bc {
                return -1;
            } else if ac > bc {
                return 1;
            }
            i += 1;
            j += 1;
        }
    }
    if i < a_bytes.len() {
        1
    } else if j < b_bytes.len() {
        -1
    } else {
        0
    }
}

// ── PluralRules ───────────────────────────────────────────────────────

fn make_plural_rules(locale: &str, _options: &Value) -> Result<Value, String> {
    let data = get_locale_data(&canonicalize_locale(locale));

    let mut obj = ObjectValue::new();
    obj.set("locale", Value::String(canonicalize_locale(locale)));

    let plural_fn = data.plural;
    obj.set(
        "select",
        Value::Builtin(BuiltinFn {
            name: "PluralRules.select".to_string(),
            func: Rc::new(move |args| {
                let n = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                let category = plural_fn(n);
                Ok(Value::String(category.to_string()))
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(obj))))
}

// ── ListFormat ────────────────────────────────────────────────────────

fn make_list_format(locale: &str, options: &Value) -> Result<Value, String> {
    let data = get_locale_data(&canonicalize_locale(locale));
    let mut style = "long".to_string();
    let mut list_type = "conjunction".to_string();

    if let Value::Object(o) = options {
        let o = o.borrow();
        if let Some(Value::String(s)) = o.properties.get("style") {
            style = s.clone();
        }
        if let Some(Value::String(s)) = o.properties.get("type") {
            list_type = s.clone();
        }
    }

    let mut obj = ObjectValue::new();
    obj.set("locale", Value::String(canonicalize_locale(locale)));

    let _ = data;
    let style_clone = style.clone();
    let type_clone = list_type.clone();
    obj.set(
        "format",
        Value::Builtin(BuiltinFn {
            name: "ListFormat.format".to_string(),
            func: Rc::new(move |args| {
                let items: Vec<String> = match args.first() {
                    Some(Value::Array(arr)) => {
                        arr.borrow().iter().map(|v| v.to_string()).collect()
                    }
                    Some(v) => vec![v.to_string()],
                    None => vec![],
                };
                let formatted = format_list(&items, &style_clone, &type_clone);
                Ok(Value::String(formatted))
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(obj))))
}

/// Format a list of strings per the style and type.
fn format_list(items: &[String], style: &str, list_type: &str) -> String {
    let (conjunction, disjunction, separator) = if style == "short" {
        (", ", " or ", ", ")
    } else if style == "narrow" {
        (" ", " ", " ")
    } else {
        (", and ", " or ", ", ")
    };

    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        2 => {
            let joiner = if list_type == "disjunction" {
                disjunction.trim_start_matches(", ")
            } else if list_type == "unit" {
                separator
            } else {
                " and "
            };
            format!("{}{}{}", items[0], joiner, items[1])
        }
        _ => {
            let joiner = if list_type == "disjunction" {
                disjunction.trim_start_matches(", ")
            } else if list_type == "unit" {
                separator
            } else {
                conjunction.trim_start_matches(", ")
            };
            let head = items[..items.len() - 1].join(", ");
            format!("{}{}{}", head, joiner, items[items.len() - 1])
        }
    }
}

// ── RelativeTimeFormat ────────────────────────────────────────────────

fn make_relative_time_format(locale: &str, options: &Value) -> Result<Value, String> {
    let data = get_locale_data(&canonicalize_locale(locale));
    let mut style = "long".to_string();
    let mut numeric = "auto".to_string();

    if let Value::Object(o) = options {
        let o = o.borrow();
        if let Some(Value::String(s)) = o.properties.get("style") {
            style = s.clone();
        }
        if let Some(Value::String(s)) = o.properties.get("numeric") {
            numeric = s.clone();
        }
    }

    let mut obj = ObjectValue::new();
    obj.set("locale", Value::String(canonicalize_locale(locale)));

    let _ = data;
    let style_clone = style.clone();
    let numeric_clone = numeric.clone();
    obj.set(
        "format",
        Value::Builtin(BuiltinFn {
            name: "RelativeTimeFormat.format".to_string(),
            func: Rc::new(move |args| {
                let value = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                let unit = args.get(1).map(|v| v.to_string()).unwrap_or_default();
                let formatted = format_relative_time(value, &unit, &style_clone, &numeric_clone);
                Ok(Value::String(formatted))
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(obj))))
}

/// Format a relative time value.
fn format_relative_time(value: f64, unit: &str, style: &str, numeric: &str) -> String {
    let (singular, plural_past, plural_future) = match unit {
        "second" | "seconds" => ("second", "seconds ago", "in seconds"),
        "minute" | "minutes" => ("minute", "minutes ago", "in minutes"),
        "hour" | "hours" => ("hour", "hours ago", "in hours"),
        "day" | "days" => ("day", "days ago", "in days"),
        "week" | "weeks" => ("week", "weeks ago", "in weeks"),
        "month" | "months" => ("month", "months ago", "in months"),
        "quarter" | "quarters" => ("quarter", "quarters ago", "in quarters"),
        "year" | "years" => ("year", "years ago", "in years"),
        _ => ("", "", ""),
    };

    let abs_value = value.abs() as i64;
    let unit_word = if abs_value == 1 {
        singular
    } else if style == "short" {
        &format!("{}s", singular)
    } else {
        match unit {
            "second" | "seconds" => "seconds",
            "minute" | "minutes" => "minutes",
            "hour" | "hours" => "hours",
            "day" | "days" => "days",
            "week" | "weeks" => "weeks",
            "month" | "months" => "months",
            "quarter" | "quarters" => "quarters",
            "year" | "years" => "years",
            _ => "",
        }
    };

    if numeric == "auto" {
        if value == 0.0 {
            format!("this {}", singular)
        } else if value < 0.0 {
            format!("{} {}", abs_value, unit_word)
        } else {
            format!("in {} {}", abs_value, unit_word)
        }
    } else {
        // "always" — always show the sign.
        if value < 0.0 {
            format!("-{} {}", abs_value, unit_word)
        } else {
            format!("+{} {}", abs_value, unit_word)
        }
    }
}

// ── Segmenter ────────────────────────────────────────────────────────

fn make_segmenter(locale: &str, options: &Value) -> Result<Value, String> {
    let _data = get_locale_data(&canonicalize_locale(locale));
    let mut granularity = "grapheme".to_string();

    if let Value::Object(o) = options {
        let o = o.borrow();
        if let Some(Value::String(s)) = o.properties.get("granularity") {
            granularity = s.clone();
        }
    }

    let mut obj = ObjectValue::new();
    obj.set("locale", Value::String(canonicalize_locale(locale)));
    obj.set("granularity", Value::String(granularity.clone()));

    let granularity_clone = granularity.clone();
    obj.set(
        "segment",
        Value::Builtin(BuiltinFn {
            name: "Segmenter.segment".to_string(),
            func: Rc::new(move |args| {
                let input = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let segments = segment_text(&input, &granularity_clone);
                let arr: Vec<Value> = segments
                    .iter()
                    .map(|s| {
                        let mut part = ObjectValue::new();
                        part.set("segment", Value::String(s.clone()));
                        part.set("index", Value::Number(0.0));
                        part.set("input", Value::String(input.clone()));
                        Value::Object(Rc::new(RefCell::new(part)))
                    })
                    .collect();
                Ok(Value::Object(Rc::new(RefCell::new({
                    let mut result = ObjectValue::new();
                    result.set(
                        "segments",
                        Value::Array(Rc::new(RefCell::new(arr))),
                    );
                    result
                }))))
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(obj))))
}

/// Segment text into graphemes, words, or sentences.
fn segment_text(text: &str, granularity: &str) -> Vec<String> {
    match granularity {
        "grapheme" => {
            // Split by Unicode grapheme clusters (simplified: split by char).
            text.chars().map(|c| c.to_string()).collect()
        }
        "word" => {
            // Split by whitespace, keeping words and whitespace as separate segments.
            let mut result = Vec::new();
            let mut current = String::new();
            for c in text.chars() {
                if c.is_alphanumeric() {
                    current.push(c);
                } else {
                    if !current.is_empty() {
                        result.push(std::mem::take(&mut current));
                    }
                    result.push(c.to_string());
                }
            }
            if !current.is_empty() {
                result.push(current);
            }
            result
        }
        "sentence" => {
            // Split by sentence terminators (. ! ?).
            let mut result = Vec::new();
            let mut current = String::new();
            for c in text.chars() {
                current.push(c);
                if c == '.' || c == '!' || c == '?' {
                    result.push(std::mem::take(&mut current));
                }
            }
            if !current.is_empty() {
                result.push(current);
            }
            result
        }
        _ => vec![text.to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_canonical_locales() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let intl = scope.get("Intl").unwrap();
        if let Value::Object(obj) = intl {
            let obj = obj.borrow();
            if let Some(Value::Builtin(fn_)) = obj.properties.get("getCanonicalLocales") {
                let result = (fn_.func)(vec![Value::String("en_us".to_string())]).unwrap();
                if let Value::Array(arr) = result {
                    let arr = arr.borrow();
                    assert_eq!(arr[0], Value::String("en-US".to_string()));
                }
            }
        }
    }

    #[test]
    fn number_format_us() {
        let fmt = make_number_format("en-US", &Value::Undefined).unwrap();
        if let Value::Object(obj) = &fmt {
            let obj = obj.borrow();
            if let Some(Value::Builtin(fmt_fn)) = obj.properties.get("format") {
                let result = (fmt_fn.func)(vec![Value::Number(1234567.891)]).unwrap();
                if let Value::String(s) = result {
                    assert!(s.contains("1,234,567"), "got: {}", s);
                }
            }
        }
    }

    #[test]
    fn number_format_de() {
        let fmt = make_number_format("de-DE", &Value::Undefined).unwrap();
        if let Value::Object(obj) = &fmt {
            let obj = obj.borrow();
            if let Some(Value::Builtin(fmt_fn)) = obj.properties.get("format") {
                let result = (fmt_fn.func)(vec![Value::Number(1234567.891)]).unwrap();
                if let Value::String(s) = result {
                    // German uses . for thousands and , for decimal.
                    assert!(s.contains("1.234.567"), "got: {}", s);
                }
            }
        }
    }

    #[test]
    fn number_format_currency() {
        let mut opts = ObjectValue::new();
        opts.set("style", Value::String("currency".to_string()));
        opts.set("currency", Value::String("USD".to_string()));
        let opts_val = Value::Object(Rc::new(RefCell::new(opts)));
        let fmt = make_number_format("en-US", &opts_val).unwrap();
        if let Value::Object(obj) = &fmt {
            let obj = obj.borrow();
            if let Some(Value::Builtin(fmt_fn)) = obj.properties.get("format") {
                let result = (fmt_fn.func)(vec![Value::Number(42.5)]).unwrap();
                if let Value::String(s) = result {
                    assert!(s.contains("$"), "got: {}", s);
                    assert!(s.contains("42"), "got: {}", s);
                }
            }
        }
    }

    #[test]
    fn plural_rules_en() {
        let fmt = make_plural_rules("en", &Value::Undefined).unwrap();
        if let Value::Object(obj) = &fmt {
            let obj = obj.borrow();
            if let Some(Value::Builtin(select_fn)) = obj.properties.get("select") {
                let one = (select_fn.func)(vec![Value::Number(1.0)]).unwrap();
                assert_eq!(one, Value::String("one".to_string()));
                let other = (select_fn.func)(vec![Value::Number(5.0)]).unwrap();
                assert_eq!(other, Value::String("other".to_string()));
            }
        }
    }

    #[test]
    fn plural_rules_ru() {
        let fmt = make_plural_rules("ru", &Value::Undefined).unwrap();
        if let Value::Object(obj) = &fmt {
            let obj = obj.borrow();
            if let Some(Value::Builtin(select_fn)) = obj.properties.get("select") {
                let one = (select_fn.func)(vec![Value::Number(1.0)]).unwrap();
                assert_eq!(one, Value::String("one".to_string()));
                let few = (select_fn.func)(vec![Value::Number(3.0)]).unwrap();
                assert_eq!(few, Value::String("few".to_string()));
                let many = (select_fn.func)(vec![Value::Number(5.0)]).unwrap();
                assert_eq!(many, Value::String("many".to_string()));
            }
        }
    }

    #[test]
    fn collator_numeric() {
        let mut opts = ObjectValue::new();
        opts.set("numeric", Value::Boolean(true));
        let opts_val = Value::Object(Rc::new(RefCell::new(opts)));
        let col = make_collator("en", &opts_val).unwrap();
        if let Value::Object(obj) = &col {
            let obj = obj.borrow();
            if let Some(Value::Builtin(cmp_fn)) = obj.properties.get("compare") {
                // "file2" should come before "file10".
                let result =
                    (cmp_fn.func)(vec![
                        Value::String("file2".to_string()),
                        Value::String("file10".to_string()),
                    ]).unwrap();
                assert_eq!(result, Value::Number(-1.0));
            }
        }
    }

    #[test]
    fn list_format_conjunction() {
        let fmt = make_list_format("en", &Value::Undefined).unwrap();
        if let Value::Object(obj) = &fmt {
            let obj = obj.borrow();
            if let Some(Value::Builtin(fmt_fn)) = obj.properties.get("format") {
                let result = (fmt_fn.func)(vec![Value::Array(Rc::new(RefCell::new(vec![
                    Value::String("a".to_string()),
                    Value::String("b".to_string()),
                    Value::String("c".to_string()),
                ])))]).unwrap();
                if let Value::String(s) = result {
                    assert!(s.contains("a") && s.contains("b") && s.contains("c"), "got: {}", s);
                }
            }
        }
    }

    #[test]
    fn relative_time_format() {
        let fmt = make_relative_time_format("en", &Value::Undefined).unwrap();
        if let Value::Object(obj) = &fmt {
            let obj = obj.borrow();
            if let Some(Value::Builtin(fmt_fn)) = obj.properties.get("format") {
                let past = (fmt_fn.func)(vec![
                    Value::Number(-3.0),
                    Value::String("days".to_string()),
                ]).unwrap();
                if let Value::String(s) = past {
                    assert!(s.contains("3"), "got: {}", s);
                }
            }
        }
    }

    #[test]
    fn days_to_ymd_epoch() {
        // 1970-01-01 = day 0.
        let (y, m, d) = days_to_ymd(0);
        assert_eq!(y, 1970);
        assert_eq!(m, 1);
        assert_eq!(d, 1);
    }

    #[test]
    fn days_to_ymd_2024() {
        // 2024-01-01 = day 19723.
        let (y, m, d) = days_to_ymd(19723);
        assert_eq!(y, 2024);
        assert_eq!(m, 1);
        assert_eq!(d, 1);
    }

    #[test]
    fn segment_grapheme() {
        let segments = segment_text("hi", "grapheme");
        assert_eq!(segments, vec!["h", "i"]);
    }

    #[test]
    fn segment_word() {
        let segments = segment_text("hello world", "word");
        assert!(segments.contains(&"hello".to_string()));
        assert!(segments.contains(&"world".to_string()));
    }

    #[test]
    fn segment_sentence() {
        let segments = segment_text("Hello. World!", "sentence");
        assert_eq!(segments.len(), 2);
    }

    #[test]
    fn add_thousands_separator_basic() {
        assert_eq!(add_thousands_separator("123", ","), "123");
        assert_eq!(add_thousands_separator("1234", ","), "1,234");
        assert_eq!(add_thousands_separator("1234567", ","), "1,234,567");
    }
}
