//! Credential Management API + Web Authentication (WebAuthn).
//!
//! # Credential Management
//!
//! ```js
//! const cred = new PasswordCredential({ id: "user", password: "pass" });
//! await navigator.credentials.store(cred);
//! const cred = await navigator.credentials.get({ password: true });
//! ```
//!
//! # WebAuthn
//!
//! ```js
//! const credential = await navigator.credentials.create({
//!   publicKey: {
//!     challenge: new Uint8Array(32),
//!     rp: { name: "Falco" },
//!     user: { id: new Uint8Array(16), name: "user", displayName: "User" },
//!     pubKeyCredParams: [{ type: "public-key", alg: -7 }],
//!   }
//! });
//! ```

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Register the Credential Management and WebAuthn APIs.
pub fn register(scope: &mut Scope) {
    register_credential_classes(scope);
    register_navigator_credentials(scope);
}

fn register_credential_classes(scope: &mut Scope) {
    // Credential (base class)
    scope.declare(
        "Credential",
        Value::Builtin(BuiltinFn {
            name: "Credential".to_string(),
            func: Rc::new(|_args| {
                let mut cred = ObjectValue::new();
                cred.set("id", Value::String(String::new()));
                cred.set("type", Value::String("credential".to_string()));
                Ok(Value::Object(Rc::new(RefCell::new(cred))))
            }),
        }),
    );

    // PasswordCredential
    scope.declare(
        "PasswordCredential",
        Value::Builtin(BuiltinFn {
            name: "PasswordCredential".to_string(),
            func: Rc::new(|args| {
                let mut cred = ObjectValue::new();
                let (id, password) = if let Some(Value::Object(data)) = args.first() {
                    let data = data.borrow();
                    let id = data.properties.get("id").map(|v| v.to_string()).unwrap_or_default();
                    let pw = data.properties.get("password").map(|v| v.to_string()).unwrap_or_default();
                    (id, pw)
                } else {
                    (String::new(), String::new())
                };
                cred.set("id", Value::String(id));
                cred.set("type", Value::String("password".to_string()));
                cred.set("password", Value::String(password));
                cred.set("name", Value::String(String::new()));
                Ok(Value::Object(Rc::new(RefCell::new(cred))))
            }),
        }),
    );

    // FederatedCredential
    scope.declare(
        "FederatedCredential",
        Value::Builtin(BuiltinFn {
            name: "FederatedCredential".to_string(),
            func: Rc::new(|args| {
                let mut cred = ObjectValue::new();
                let (id, provider) = if let Some(Value::Object(data)) = args.first() {
                    let data = data.borrow();
                    let id = data.properties.get("id").map(|v| v.to_string()).unwrap_or_default();
                    let prov = data.properties.get("provider").map(|v| v.to_string()).unwrap_or_default();
                    (id, prov)
                } else {
                    (String::new(), String::new())
                };
                cred.set("id", Value::String(id));
                cred.set("type", Value::String("federated".to_string()));
                cred.set("provider", Value::String(provider));
                cred.set("name", Value::String(String::new()));
                cred.set("protocol", Value::String(String::new()));
                Ok(Value::Object(Rc::new(RefCell::new(cred))))
            }),
        }),
    );

    // PublicKeyCredential (WebAuthn)
    scope.declare(
        "PublicKeyCredential",
        Value::Builtin(BuiltinFn {
            name: "PublicKeyCredential".to_string(),
            func: Rc::new(|_args| {
                let mut cred = ObjectValue::new();
                cred.set("id", Value::String(String::new()));
                cred.set("type", Value::String("public-key".to_string()));
                cred.set("rawId", Value::Array(Rc::new(RefCell::new(vec![]))));
                cred.set("authenticatorAttachment", Value::String("platform".to_string()));
                // response is an AuthenticatorAttestationResponse or AuthenticatorAssertionResponse.
                let mut response = ObjectValue::new();
                response.set("clientDataJSON", Value::Array(Rc::new(RefCell::new(vec![]))));
                response.set("attestationObject", Value::Array(Rc::new(RefCell::new(vec![]))));
                response.set(
                    "getTransports",
                    Value::Builtin(BuiltinFn {
                        name: "AuthenticatorAttestationResponse.getTransports".to_string(),
                        func: Rc::new(|_args| {
                            Ok(Value::Array(Rc::new(RefCell::new(vec![
                                Value::String("internal".to_string()),
                            ]))))
                        }),
                    }),
                );
                cred.set("response", Value::Object(Rc::new(RefCell::new(response))));
                Ok(Value::Object(Rc::new(RefCell::new(cred))))
            }),
        }),
    );

    // PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable()
    scope.declare(
        "PublicKeyCredential_isUserVerifyingPlatformAuthenticatorAvailable",
        Value::Builtin(BuiltinFn {
            name: "PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable".to_string(),
            func: Rc::new(|_args| Ok(Value::Boolean(true))),
        }),
    );

    // PublicKeyCredential.isConditionalMediationAvailable()
    scope.declare(
        "PublicKeyCredential_isConditionalMediationAvailable",
        Value::Builtin(BuiltinFn {
            name: "PublicKeyCredential.isConditionalMediationAvailable".to_string(),
            func: Rc::new(|_args| Ok(Value::Boolean(false))),
        }),
    );
}

fn register_navigator_credentials(scope: &mut Scope) {
    let nav_val = scope.get("navigator");
    let mut nav = if let Some(Value::Object(nav_rc)) = nav_val {
        let n = nav_rc.borrow();
        let mut copy = ObjectValue::new();
        for (k, v) in n.properties.iter() {
            copy.properties.insert(k.clone(), v.clone());
        }
        copy.prototype = n.prototype.clone();
        copy
    } else {
        ObjectValue::new()
    };

    // navigator.credentials
    let mut credentials = ObjectValue::new();

    // get(options) — retrieves a credential.
    credentials.set(
        "get",
        Value::Builtin(BuiltinFn {
            name: "credentials.get".to_string(),
            func: Rc::new(|_args| {
                // Return null (no credential found).
                Ok(Value::Null)
            }),
        }),
    );

    // store(credential) — stores a credential.
    credentials.set(
        "store",
        Value::Builtin(BuiltinFn {
            name: "credentials.store".to_string(),
            func: Rc::new(|args| Ok(args.first().cloned().unwrap_or(Value::Undefined))),
        }),
    );

    // create(options) — creates a new credential (used by WebAuthn).
    credentials.set(
        "create",
        Value::Builtin(BuiltinFn {
            name: "credentials.create".to_string(),
            func: Rc::new(|args| {
                // Check if this is a WebAuthn request (has publicKey).
                let is_webauthn = if let Some(Value::Object(opts)) = &args.first() {
                    opts.borrow().properties.contains_key("publicKey")
                } else {
                    false
                };

                if is_webauthn {
                    // Return a mock PublicKeyCredential.
                    let mut cred = ObjectValue::new();
                    cred.set("id", Value::String("mock-cred-id".to_string()));
                    cred.set("type", Value::String("public-key".to_string()));
                    cred.set("rawId", Value::Array(Rc::new(RefCell::new(vec![]))));
                    cred.set("authenticatorAttachment", Value::String("platform".to_string()));

                    let mut response = ObjectValue::new();
                    response.set("clientDataJSON", Value::Array(Rc::new(RefCell::new(vec![]))));
                    response.set("attestationObject", Value::Array(Rc::new(RefCell::new(vec![]))));
                    response.set(
                        "getTransports",
                        Value::Builtin(BuiltinFn {
                            name: "AuthenticatorResponse.getTransports".to_string(),
                            func: Rc::new(|_args| {
                                Ok(Value::Array(Rc::new(RefCell::new(vec![
                                    Value::String("internal".to_string()),
                                ]))))
                            }),
                        }),
                    );
                    response.set(
                        "getAuthenticatorData",
                        Value::Builtin(BuiltinFn {
                            name: "AuthenticatorResponse.getAuthenticatorData".to_string(),
                            func: Rc::new(|_args| Ok(Value::Array(Rc::new(RefCell::new(vec![]))))),
                        }),
                    );
                    response.set(
                        "getPublicKey",
                        Value::Builtin(BuiltinFn {
                            name: "AuthenticatorResponse.getPublicKey".to_string(),
                            func: Rc::new(|_args| Ok(Value::Array(Rc::new(RefCell::new(vec![]))))),
                        }),
                    );
                    response.set(
                        "getPublicKeyAlgorithm",
                        Value::Builtin(BuiltinFn {
                            name: "AuthenticatorResponse.getPublicKeyAlgorithm".to_string(),
                            func: Rc::new(|_args| Ok(Value::Number(-7.0))), // ES256
                        }),
                    );
                    cred.set("response", Value::Object(Rc::new(RefCell::new(response))));
                    Ok(Value::Object(Rc::new(RefCell::new(cred))))
                } else {
                    // PasswordCredential or FederatedCredential creation.
                    Ok(args.first().cloned().unwrap_or(Value::Undefined))
                }
            }),
        }),
    );

    // preventSilentAccess()
    credentials.set(
        "preventSilentAccess",
        Value::Builtin(BuiltinFn {
            name: "credentials.preventSilentAccess".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    nav.set("credentials", Value::Object(Rc::new(RefCell::new(credentials))));
    scope.declare("navigator", Value::Object(Rc::new(RefCell::new(nav))));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_credential_creation() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("PasswordCredential").unwrap();
        if let Value::Builtin(b) = ctor {
            let mut data = ObjectValue::new();
            data.set("id", Value::String("user123".to_string()));
            data.set("password", Value::String("secret".to_string()));
            let cred = (b.func)(vec![Value::Object(Rc::new(RefCell::new(data)))]).unwrap();
            if let Value::Object(c) = cred {
                let c = c.borrow();
                assert_eq!(c.properties.get("id"), Some(&Value::String("user123".to_string())));
                assert_eq!(c.properties.get("type"), Some(&Value::String("password".to_string())));
            }
        }
    }

    #[test]
    fn federated_credential_creation() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("FederatedCredential").unwrap();
        if let Value::Builtin(b) = ctor {
            let mut data = ObjectValue::new();
            data.set("id", Value::String("user".to_string()));
            data.set("provider", Value::String("https://accounts.google.com".to_string()));
            let cred = (b.func)(vec![Value::Object(Rc::new(RefCell::new(data)))]).unwrap();
            if let Value::Object(c) = cred {
                let c = c.borrow();
                assert_eq!(c.properties.get("type"), Some(&Value::String("federated".to_string())));
            }
        }
    }

    #[test]
    fn public_key_credential_creation() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("PublicKeyCredential").unwrap();
        if let Value::Builtin(b) = ctor {
            let cred = (b.func)(vec![]).unwrap();
            if let Value::Object(c) = cred {
                let c = c.borrow();
                assert_eq!(c.properties.get("type"), Some(&Value::String("public-key".to_string())));
                assert!(c.properties.contains_key("response"));
                assert!(c.properties.contains_key("rawId"));
            }
        }
    }

    #[test]
    fn navigator_credentials_exists() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            let nav_obj = nav_obj.borrow();
            assert!(nav_obj.properties.contains_key("credentials"));
        }
    }

    #[test]
    fn credentials_store() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            let nav_obj = nav_obj.borrow();
            if let Some(Value::Object(creds)) = nav_obj.properties.get("credentials") {
                let creds = creds.borrow();
                if let Some(Value::Builtin(store_fn)) = creds.properties.get("store") {
                    let cred = Value::String("test-cred".to_string());
                    let result = (store_fn.func)(vec![cred.clone()]).unwrap();
                    assert_eq!(result, cred);
                }
            }
        }
    }

    #[test]
    fn webauthn_create() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            let nav_obj = nav_obj.borrow();
            if let Some(Value::Object(creds)) = nav_obj.properties.get("credentials") {
                let creds = creds.borrow();
                if let Some(Value::Builtin(create_fn)) = creds.properties.get("create") {
                    let mut opts = ObjectValue::new();
                    let mut pk = ObjectValue::new();
                    pk.set("challenge", Value::Array(Rc::new(RefCell::new(vec![]))));
                    pk.set("rp", Value::Object(Rc::new(RefCell::new(ObjectValue::new()))));
                    opts.set("publicKey", Value::Object(Rc::new(RefCell::new(pk))));
                    let result = (create_fn.func)(vec![Value::Object(Rc::new(RefCell::new(opts)))]).unwrap();
                    if let Value::Object(cred) = result {
                        let cred = cred.borrow();
                        assert_eq!(cred.properties.get("type"), Some(&Value::String("public-key".to_string())));
                    }
                }
            }
        }
    }

    #[test]
    fn is_user_verifying_platform_authenticator_available() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let fn_val = scope.get("PublicKeyCredential_isUserVerifyingPlatformAuthenticatorAvailable").unwrap();
        if let Value::Builtin(b) = fn_val {
            let result = (b.func)(vec![]).unwrap();
            assert_eq!(result, Value::Boolean(true));
        }
    }
}
