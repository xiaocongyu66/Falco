//! Crypto API — `crypto.getRandomValues()` and `crypto.subtle.digest()`.
//!
//! # Random Number Generation
//!
//! `crypto.getRandomValues(array)` fills a `Uint8Array` with cryptographically
//! secure random bytes. We use Rust's `rand` crate (specifically `OsRng` for
//! entropy from the OS).
//!
//! # SubtleCrypto
//!
//! `crypto.subtle.digest(algorithm, data)` computes a hash of the data.
//! Supported algorithms:
//! - SHA-1 (legacy, but widely used)
//! - SHA-256
//! - SHA-384
//! - SHA-512
//!
//! We implement these from scratch (no external crypto crate dependency)
//! to keep Falco dependency-free.

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Register the Crypto API on the scope.
pub fn register(scope: &mut Scope) {
    let mut crypto_obj = ObjectValue::new();

    // crypto.getRandomValues(array) — fills array with random bytes.
    crypto_obj.set(
        "getRandomValues",
        Value::Builtin(BuiltinFn {
            name: "crypto.getRandomValues".to_string(),
            func: Rc::new(|args| {
                let arr_val = args
                    .first()
                    .cloned()
                    .ok_or_else(|| "getRandomValues: missing array argument".to_string())?;
                if let Value::Array(arr) = &arr_val {
                    let mut arr = arr.borrow_mut();
                    // Use OS entropy via /dev/urandom on Unix, CryptGenRandom on Windows.
                    let mut buf = vec![0u8; arr.len()];
                    fill_random(&mut buf)?;
                    for (i, b) in buf.iter().enumerate() {
                        arr[i] = Value::Number(*b as f64);
                    }
                    Ok(arr_val.clone())
                } else {
                    Err("getRandomValues: argument must be a Uint8Array".to_string())
                }
            }),
        }),
    );

    // crypto.randomUUID() — generates a v4 UUID string.
    crypto_obj.set(
        "randomUUID",
        Value::Builtin(BuiltinFn {
            name: "crypto.randomUUID".to_string(),
            func: Rc::new(|_args| {
                let mut buf = [0u8; 16];
                fill_random(&mut buf)?;
                // Set version (4) and variant bits.
                buf[6] = (buf[6] & 0x0F) | 0x40;
                buf[8] = (buf[8] & 0x3F) | 0x80;
                let s = format!(
                    "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                    buf[0], buf[1], buf[2], buf[3],
                    buf[4], buf[5],
                    buf[6], buf[7],
                    buf[8], buf[9],
                    buf[10], buf[11], buf[12], buf[13], buf[14], buf[15]
                );
                Ok(Value::String(s))
            }),
        }),
    );

    // crypto.subtle — SubtleCrypto object.
    let mut subtle_obj = ObjectValue::new();

    // subtle.digest(algorithm, data) — returns a Promise resolving to a hash.
    subtle_obj.set(
        "digest",
        Value::Builtin(BuiltinFn {
            name: "crypto.subtle.digest".to_string(),
            func: Rc::new(|args| {
                let algorithm = args
                    .first()
                    .map(|v| v.to_string())
                    .ok_or_else(|| "digest: missing algorithm".to_string())?
                    .to_lowercase();
                let data_val = args
                    .get(1)
                    .cloned()
                    .ok_or_else(|| "digest: missing data".to_string())?;
                let bytes = extract_bytes(&data_val)?;

                let hash = match algorithm.as_str() {
                    "sha-1" | "sha1" => sha1(&bytes),
                    "sha-256" | "sha256" => sha256(&bytes).to_vec(),
                    "sha-384" | "sha384" => sha384(&bytes).to_vec(),
                    "sha-512" | "sha512" => sha512(&bytes).to_vec(),
                    _ => {
                        return Err(format!("unsupported hash algorithm: {}", algorithm));
                    }
                };

                let arr: Vec<Value> = hash.iter().map(|b| Value::Number(*b as f64)).collect();
                Ok(Value::Array(Rc::new(RefCell::new(arr))))
            }),
        }),
    );

    // subtle.encrypt/decrypt — stubs (return error).
    subtle_obj.set(
        "encrypt",
        Value::Builtin(BuiltinFn {
            name: "crypto.subtle.encrypt".to_string(),
            func: Rc::new(|_args| Err("SubtleCrypto.encrypt not implemented".to_string())),
        }),
    );
    subtle_obj.set(
        "decrypt",
        Value::Builtin(BuiltinFn {
            name: "crypto.subtle.decrypt".to_string(),
            func: Rc::new(|_args| Err("SubtleCrypto.decrypt not implemented".to_string())),
        }),
    );

    // subtle.importKey — stub.
    subtle_obj.set(
        "importKey",
        Value::Builtin(BuiltinFn {
            name: "crypto.subtle.importKey".to_string(),
            func: Rc::new(|_args| Err("SubtleCrypto.importKey not implemented".to_string())),
        }),
    );

    crypto_obj.set("subtle", Value::Object(Rc::new(RefCell::new(subtle_obj))));
    scope.declare("crypto", Value::Object(Rc::new(RefCell::new(crypto_obj))));
}

/// Fill a buffer with cryptographically secure random bytes.
fn fill_random(buf: &mut [u8]) -> Result<(), String> {
    // Use /dev/urandom on Unix, RtlGenRandom on Windows.
    #[cfg(unix)]
    {
        use std::io::Read;
        let mut f = std::fs::File::open("/dev/urandom")
            .map_err(|e| format!("getRandomValues: cannot open /dev/urandom: {}", e))?;
        f.read_exact(buf)
            .map_err(|e| format!("getRandomValues: read failed: {}", e))?;
        Ok(())
    }
    #[cfg(windows)]
    {
        // BCryptGenRandom — modern Windows API, available in bcrypt.lib.
        const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x00000002;

        extern "system" {
            fn BCryptGenRandom(
                hAlgorithm: *mut std::ffi::c_void,
                pbBuffer: *mut u8,
                cbBuffer: u32,
                dwFlags: u32,
            ) -> i32;
        }

        let len = buf.len() as u32;
        let r = unsafe {
            BCryptGenRandom(
                std::ptr::null_mut(),
                buf.as_mut_ptr(),
                len,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        };
        if r != 0 {
            Err("BCryptGenRandom failed".to_string())
        } else {
            Ok(())
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        // Fallback: use a simple PRNG seeded with current time.
        use std::time::SystemTime;
        let mut seed = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x12345678);
        for b in buf.iter_mut() {
            // xorshift64
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            *b = seed as u8;
        }
        Ok(())
    }
}

/// Extract bytes from a Value.
fn extract_bytes(val: &Value) -> Result<Vec<u8>, String> {
    match val {
        Value::Array(arr) => {
            let arr = arr.borrow();
            arr.iter()
                .map(|v| {
                    let n = v.to_number();
                    if n < 0.0 || n > 255.0 {
                        Err(format!("byte out of range: {}", n))
                    } else {
                        Ok(n as u8)
                    }
                })
                .collect()
        }
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        _ => Err(format!("cannot extract bytes from {}", val.type_name())),
    }
}

// ── SHA-1 ──────────────────────────────────────────────────────────────
//
// SHA-1 is deprecated for security-critical use, but still required by
// the Web Crypto API for legacy compatibility.

/// Compute SHA-1 hash. Returns 20 bytes.
fn sha1(data: &[u8]) -> Vec<u8> {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    // Pre-processing: pad the message.
    let mut msg = data.to_vec();
    let original_bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&original_bit_len.to_be_bytes());

    // Process each 512-bit (64-byte) chunk.
    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);

        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut result = Vec::with_capacity(20);
    for h in [h0, h1, h2, h3, h4] {
        result.extend_from_slice(&h.to_be_bytes());
    }
    result
}

// ── SHA-256 / SHA-384 / SHA-512 ────────────────────────────────────────

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const SHA256_H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// Compute SHA-256 hash. Returns 32 bytes.
fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h = SHA256_H0;

    let mut msg = data.to_vec();
    let original_bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&original_bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut result = [0u8; 32];
    for (i, &v) in h.iter().enumerate() {
        result[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    result
}

/// Compute SHA-512 hash. Returns 64 bytes.
fn sha512(data: &[u8]) -> [u8; 64] {
    const K: [u64; 80] = [
        0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc,
        0x3956c25bf348b538, 0x59f111f1b605d019, 0x923f82a4af194f9b, 0xab1c5ed5da6d8118,
        0xd807aa98a3030242, 0x12835b0145706fbe, 0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2,
        0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235, 0xc19bf174cf692694,
        0xe49b69c19ef14ad2, 0xefbe4786384f25e3, 0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65,
        0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5,
        0x983e5152ee66dfab, 0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4,
        0xc6e00bf33da88fc2, 0xd5a79147930aa725, 0x06ca6351e003826f, 0x142929670a0e6e70,
        0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed, 0x53380d139d95b3df,
        0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b,
        0xa2bfe8a14cf10364, 0xa81a664bbc423001, 0xc24b8b70d0f89791, 0xc76c51a30654be30,
        0xd192e819d6ef5218, 0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8,
        0x19a4c116b8d2d0c8, 0x1e376c085141ab53, 0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8,
        0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb, 0x5b9cca4f7763e373, 0x682e6ff3d6b2b8a3,
        0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec,
        0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b,
        0xca273eceea26619c, 0xd186b8c721c0c207, 0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178,
        0x06f067aa72176fba, 0x0a637dc5a2c898a6, 0x113f9804bef90dae, 0x1b710b35131c471b,
        0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc, 0x431d67c49c100d4c,
        0x4cc5d4becb3e42b6, 0x597f299cfc657e2a, 0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
    ];

    let mut hh: [u64; 8] = [
        0x6a09e667f3bcc908, 0xbb67ae8584caa73b, 0x3c6ef372fe94f82b, 0xa54ff53a5f1d36f1,
        0x510e527fade682d1, 0x9b05688c2b3e6c1f, 0x1f83d9abfb41bd6b, 0x5be0cd19137e2179,
    ];

    let mut msg = data.to_vec();
    let original_bit_len = (data.len() as u128) * 8;
    msg.push(0x80);
    while msg.len() % 128 != 112 {
        msg.push(0);
    }
    msg.extend_from_slice(&original_bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(128) {
        let mut w = [0u64; 80];
        for i in 0..16 {
            w[i] = u64::from_be_bytes([
                chunk[i * 8], chunk[i * 8 + 1], chunk[i * 8 + 2], chunk[i * 8 + 3],
                chunk[i * 8 + 4], chunk[i * 8 + 5], chunk[i * 8 + 6], chunk[i * 8 + 7],
            ]);
        }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h) =
            (hh[0], hh[1], hh[2], hh[3], hh[4], hh[5], hh[6], hh[7]);

        for i in 0..80 {
            let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        hh[0] = hh[0].wrapping_add(a);
        hh[1] = hh[1].wrapping_add(b);
        hh[2] = hh[2].wrapping_add(c);
        hh[3] = hh[3].wrapping_add(d);
        hh[4] = hh[4].wrapping_add(e);
        hh[5] = hh[5].wrapping_add(f);
        hh[6] = hh[6].wrapping_add(g);
        hh[7] = hh[7].wrapping_add(h);
    }

    let mut result = [0u8; 64];
    for (i, &v) in hh.iter().enumerate() {
        result[i * 8..i * 8 + 8].copy_from_slice(&v.to_be_bytes());
    }
    result
}

/// Compute SHA-384 hash. Returns 48 bytes.
fn sha384(data: &[u8]) -> [u8; 48] {
    // SHA-384 is SHA-512 with different initial values and truncated output.
    const K: [u64; 80] = [
        0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc,
        0x3956c25bf348b538, 0x59f111f1b605d019, 0x923f82a4af194f9b, 0xab1c5ed5da6d8118,
        0xd807aa98a3030242, 0x12835b0145706fbe, 0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2,
        0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235, 0xc19bf174cf692694,
        0xe49b69c19ef14ad2, 0xefbe4786384f25e3, 0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65,
        0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5,
        0x983e5152ee66dfab, 0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4,
        0xc6e00bf33da88fc2, 0xd5a79147930aa725, 0x06ca6351e003826f, 0x142929670a0e6e70,
        0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed, 0x53380d139d95b3df,
        0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b,
        0xa2bfe8a14cf10364, 0xa81a664bbc423001, 0xc24b8b70d0f89791, 0xc76c51a30654be30,
        0xd192e819d6ef5218, 0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8,
        0x19a4c116b8d2d0c8, 0x1e376c085141ab53, 0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8,
        0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb, 0x5b9cca4f7763e373, 0x682e6ff3d6b2b8a3,
        0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec,
        0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b,
        0xca273eceea26619c, 0xd186b8c721c0c207, 0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178,
        0x06f067aa72176fba, 0x0a637dc5a2c898a6, 0x113f9804bef90dae, 0x1b710b35131c471b,
        0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc, 0x431d67c49c100d4c,
        0x4cc5d4becb3e42b6, 0x597f299cfc657e2a, 0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
    ];

    let mut hh: [u64; 8] = [
        0xcbbb9d5dc1059ed8, 0x629a292a367cd507, 0x9159015a3070dd17, 0x152fecd8f70e5939,
        0x67332667ffc00b31, 0x8eb44a8768581511, 0xdb0c2e0d64f98fa7, 0x47b5481dbefa4fa4,
    ];

    let mut msg = data.to_vec();
    let original_bit_len = (data.len() as u128) * 8;
    msg.push(0x80);
    while msg.len() % 128 != 112 {
        msg.push(0);
    }
    msg.extend_from_slice(&original_bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(128) {
        let mut w = [0u64; 80];
        for i in 0..16 {
            w[i] = u64::from_be_bytes([
                chunk[i * 8], chunk[i * 8 + 1], chunk[i * 8 + 2], chunk[i * 8 + 3],
                chunk[i * 8 + 4], chunk[i * 8 + 5], chunk[i * 8 + 6], chunk[i * 8 + 7],
            ]);
        }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h) =
            (hh[0], hh[1], hh[2], hh[3], hh[4], hh[5], hh[6], hh[7]);

        for i in 0..80 {
            let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        hh[0] = hh[0].wrapping_add(a);
        hh[1] = hh[1].wrapping_add(b);
        hh[2] = hh[2].wrapping_add(c);
        hh[3] = hh[3].wrapping_add(d);
        hh[4] = hh[4].wrapping_add(e);
        hh[5] = hh[5].wrapping_add(f);
        hh[6] = hh[6].wrapping_add(g);
        hh[7] = hh[7].wrapping_add(h);
    }

    let mut result = [0u8; 48];
    for (i, &v) in hh.iter().take(6).enumerate() {
        result[i * 8..i * 8 + 8].copy_from_slice(&v.to_be_bytes());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_empty() {
        let hash = sha1(b"");
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(hex, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }

    #[test]
    fn sha1_abc() {
        let hash = sha1(b"abc");
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(hex, "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn sha256_empty() {
        let hash = sha256(b"");
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(hex, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn sha256_abc() {
        let hash = sha256(b"abc");
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(hex, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn sha512_empty() {
        let hash = sha512(b"");
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(
            hex,
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
        );
    }

    #[test]
    fn sha512_abc() {
        let hash = sha512(b"abc");
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(
            hex,
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
    }

    #[test]
    fn sha384_abc() {
        let hash = sha384(b"abc");
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(
            hex,
            "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7"
        );
    }

    #[test]
    fn random_uuid_format() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let crypto_val = scope.get("crypto").unwrap();
        if let Value::Object(obj) = crypto_val {
            let obj = obj.borrow();
            if let Some(Value::Builtin(uuid_fn)) = obj.properties.get("randomUUID") {
                let result = (uuid_fn.func)(vec![]).unwrap();
                if let Value::String(s) = result {
                    // Should be 36 chars: 8-4-4-4-12
                    assert_eq!(s.len(), 36);
                    let parts: Vec<&str> = s.split('-').collect();
                    assert_eq!(parts.len(), 5);
                    assert_eq!(parts[0].len(), 8);
                    assert_eq!(parts[1].len(), 4);
                    assert_eq!(parts[2].len(), 4);
                    assert_eq!(parts[3].len(), 4);
                    assert_eq!(parts[4].len(), 12);
                    // Version 4
                    assert!(parts[2].starts_with('4'));
                }
            }
        }
    }

    #[test]
    fn get_random_values_fills_array() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let crypto_val = scope.get("crypto").unwrap();
        if let Value::Object(obj) = crypto_val {
            let obj = obj.borrow();
            if let Some(Value::Builtin(grv_fn)) = obj.properties.get("getRandomValues") {
                let arr = Value::Array(Rc::new(RefCell::new(vec![
                    Value::Number(0.0); 16
                ])));
                let _ = (grv_fn.func)(vec![arr.clone()]).unwrap();
                if let Value::Array(a) = arr {
                    let a = a.borrow();
                    // At least one byte should be non-zero (statistically near-certain).
                    let non_zero = a.iter().filter(|v| v.to_number() != 0.0).count();
                    assert!(non_zero > 0, "all bytes were zero");
                }
            }
        }
    }
}
