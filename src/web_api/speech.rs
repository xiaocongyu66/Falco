//! Web Speech API — speech recognition and synthesis.
//!
//! # SpeechSynthesis (Text-to-Speech)
//!
//! ```js
//! const utterance = new SpeechSynthesisUtterance("Hello world");
//! speechSynthesis.speak(utterance);
//! ```
//!
//! Since Falco doesn't have an audio output backend, speech synthesis
//! is a no-op (the speak call succeeds but produces no sound).
//!
//! # SpeechRecognition (Speech-to-Text)
//!
//! ```js
//! const recognition = new SpeechRecognition();
//! recognition.onresult = (e) => console.log(e.results[0][0].transcript);
//! recognition.start();
//! ```
//!
//! Since Falco doesn't have a microphone backend, recognition always
//! returns empty results.

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Register the Web Speech API.
pub fn register(scope: &mut Scope) {
    register_synthesis(scope);
    register_recognition(scope);
}

fn register_synthesis(scope: &mut Scope) {
    // SpeechSynthesisUtterance constructor.
    scope.declare(
        "SpeechSynthesisUtterance",
        Value::Builtin(BuiltinFn {
            name: "SpeechSynthesisUtterance".to_string(),
            func: Rc::new(|args| {
                let text = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let mut utterance = ObjectValue::new();
                utterance.set("text", Value::String(text));
                utterance.set("lang", Value::String("en-US".to_string()));
                utterance.set("voice", Value::Null);
                utterance.set("volume", Value::Number(1.0));
                utterance.set("rate", Value::Number(1.0));
                utterance.set("pitch", Value::Number(1.0));
                utterance.set("onstart", Value::Null);
                utterance.set("onend", Value::Null);
                utterance.set("onerror", Value::Null);
                utterance.set("onpause", Value::Null);
                utterance.set("onresume", Value::Null);
                utterance.set("onmark", Value::Null);
                utterance.set("onboundary", Value::Null);
                Ok(Value::Object(Rc::new(RefCell::new(utterance))))
            }),
        }),
    );

    // SpeechSynthesisVoice constructor.
    scope.declare(
        "SpeechSynthesisVoice",
        Value::Builtin(BuiltinFn {
            name: "SpeechSynthesisVoice".to_string(),
            func: Rc::new(|_args| {
                let mut voice = ObjectValue::new();
                voice.set("voiceURI", Value::String("default".to_string()));
                voice.set("name", Value::String("Default Voice".to_string()));
                voice.set("lang", Value::String("en-US".to_string()));
                voice.set("localService", Value::Boolean(true));
                voice.set("default", Value::Boolean(true));
                Ok(Value::Object(Rc::new(RefCell::new(voice))))
            }),
        }),
    );

    // speechSynthesis global object.
    let mut synth = ObjectValue::new();
    synth.set("pending", Value::Boolean(false));
    synth.set("speaking", Value::Boolean(false));
    synth.set("paused", Value::Boolean(false));

    synth.set(
        "speak",
        Value::Builtin(BuiltinFn {
            name: "speechSynthesis.speak".to_string(),
            func: Rc::new(|args| {
                // Log the text (no actual audio output).
                if let Some(Value::Object(u)) = args.first() {
                    if let Some(Value::String(text)) = u.borrow().properties.get("text") {
                        eprintln!("[speech] {}", text);
                    }
                }
                Ok(Value::Undefined)
            }),
        }),
    );

    synth.set(
        "cancel",
        Value::Builtin(BuiltinFn {
            name: "speechSynthesis.cancel".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    synth.set(
        "pause",
        Value::Builtin(BuiltinFn {
            name: "speechSynthesis.pause".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    synth.set(
        "resume",
        Value::Builtin(BuiltinFn {
            name: "speechSynthesis.resume".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    synth.set(
        "getVoices",
        Value::Builtin(BuiltinFn {
            name: "speechSynthesis.getVoices".to_string(),
            func: Rc::new(|_args| {
                // Return a single default voice.
                let mut voice = ObjectValue::new();
                voice.set("voiceURI", Value::String("default".to_string()));
                voice.set("name", Value::String("Default Voice".to_string()));
                voice.set("lang", Value::String("en-US".to_string()));
                voice.set("localService", Value::Boolean(true));
                voice.set("default", Value::Boolean(true));
                Ok(Value::Array(Rc::new(RefCell::new(vec![
                    Value::Object(Rc::new(RefCell::new(voice))),
                ]))))
            }),
        }),
    );

    synth.set(
        "onvoiceschanged",
        Value::Null,
    );

    scope.declare("speechSynthesis", Value::Object(Rc::new(RefCell::new(synth))));
}

fn register_recognition(scope: &mut Scope) {
    // SpeechRecognition constructor.
    let recognition_ctor = Value::Builtin(BuiltinFn {
        name: "SpeechRecognition".to_string(),
        func: Rc::new(|_args| {
            let mut recog = ObjectValue::new();
            recog.set("lang", Value::String("en-US".to_string()));
            recog.set("continuous", Value::Boolean(false));
            recog.set("interimResults", Value::Boolean(false));
            recog.set("maxAlternatives", Value::Number(1.0));
            recog.set("onresult", Value::Null);
            recog.set("onerror", Value::Null);
            recog.set("onend", Value::Null);
            recog.set("onstart", Value::Null);

            recog.set(
                "start",
                Value::Builtin(BuiltinFn {
                    name: "SpeechRecognition.start".to_string(),
                    func: Rc::new(|_args| Ok(Value::Undefined)),
                }),
            );

            recog.set(
                "stop",
                Value::Builtin(BuiltinFn {
                    name: "SpeechRecognition.stop".to_string(),
                    func: Rc::new(|_args| Ok(Value::Undefined)),
                }),
            );

            recog.set(
                "abort",
                Value::Builtin(BuiltinFn {
                    name: "SpeechRecognition.abort".to_string(),
                    func: Rc::new(|_args| Ok(Value::Undefined)),
                }),
            );

            Ok(Value::Object(Rc::new(RefCell::new(recog))))
        }),
    });

    scope.declare("SpeechRecognition", recognition_ctor.clone());
    scope.declare("webkitSpeechRecognition", recognition_ctor);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speech_synthesis_utterance() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("SpeechSynthesisUtterance").unwrap();
        if let Value::Builtin(b) = ctor {
            let u = (b.func)(vec![Value::String("Hello".to_string())]).unwrap();
            if let Value::Object(obj) = u {
                let obj = obj.borrow();
                assert_eq!(obj.properties.get("text"), Some(&Value::String("Hello".to_string())));
                assert_eq!(obj.properties.get("volume"), Some(&Value::Number(1.0)));
            }
        }
    }

    #[test]
    fn speech_synthesis_global() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let synth = scope.get("speechSynthesis").unwrap();
        if let Value::Object(obj) = synth {
            let obj = obj.borrow();
            assert!(obj.properties.contains_key("speak"));
            assert!(obj.properties.contains_key("cancel"));
            assert!(obj.properties.contains_key("getVoices"));
        }
    }

    #[test]
    fn get_voices() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let synth = scope.get("speechSynthesis").unwrap();
        if let Value::Object(obj) = synth {
            let obj = obj.borrow();
            if let Some(Value::Builtin(get_voices_fn)) = obj.properties.get("getVoices") {
                let result = (get_voices_fn.func)(vec![]).unwrap();
                if let Value::Array(arr) = result {
                    let arr = arr.borrow();
                    assert_eq!(arr.len(), 1);
                }
            }
        }
    }

    #[test]
    fn speech_recognition_creation() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("SpeechRecognition").unwrap();
        if let Value::Builtin(b) = ctor {
            let recog = (b.func)(vec![]).unwrap();
            if let Value::Object(obj) = recog {
                let obj = obj.borrow();
                assert!(obj.properties.contains_key("start"));
                assert!(obj.properties.contains_key("stop"));
                assert!(obj.properties.contains_key("abort"));
                assert_eq!(obj.properties.get("lang"), Some(&Value::String("en-US".to_string())));
            }
        }
    }

    #[test]
    fn webkit_speech_recognition_alias() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        assert!(scope.get("webkitSpeechRecognition").is_some());
    }
}
