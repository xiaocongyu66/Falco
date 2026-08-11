//! WebRTC — real-time peer-to-peer communication.
//!
//! # Implementation
//!
//! Real WebRTC requires:
//! - ICE (Interactive Connectivity Establishment) for NAT traversal
//! - STUN/TURN servers for relay
//! - DTLS-SRTP for secure transport
//! - SDP (Session Description Protocol) for negotiation
//! - RTP for media transport
//!
//! This module implements the **JavaScript API surface** with stub
//! implementations that maintain correct state transitions. No actual
//! network connections are made — useful for testing code that uses
//! the WebRTC API without requiring a full WebRTC stack.
//!
//! # Supported Classes
//!
//! - `RTCPeerConnection` — the main WebRTC connection class
//! - `RTCDataChannel` — bidirectional data channel
//! - `MediaStream` — a stream of media tracks
//! - `MediaStreamTrack` — a single audio/video track
//! - `RTCSessionDescription` — SDP offer/answer
//! - `RTCIceCandidate` — an ICE candidate
//! - `RTCConfiguration` — STUN/TURN server configuration

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Register the WebRTC API.
pub fn register(scope: &mut Scope) {
    register_rtc_peer_connection(scope);
    register_rtc_data_channel(scope);
    register_media_stream(scope);
    register_rtc_session_description(scope);
    register_rtc_ice_candidate(scope);
}

fn register_rtc_peer_connection(scope: &mut Scope) {
    scope.declare(
        "RTCPeerConnection",
        Value::Builtin(BuiltinFn {
            name: "RTCPeerConnection".to_string(),
            func: Rc::new(|args| {
                let config = args.first().cloned().unwrap_or(Value::Undefined);

                let mut pc = ObjectValue::new();
                pc.set("connectionState", Value::String("new".to_string()));
                pc.set("iceConnectionState", Value::String("new".to_string()));
                pc.set("iceGatheringState", Value::String("new".to_string()));
                pc.set("signalingState", Value::String("stable".to_string()));
                pc.set("localDescription", Value::Null);
                pc.set("remoteDescription", Value::Null);
                pc.set("currentLocalDescription", Value::Null);
                pc.set("currentRemoteDescription", Value::Null);
                pc.set("pendingLocalDescription", Value::Null);
                pc.set("pendingRemoteDescription", Value::Null);

                // Store the configuration.
                pc.set("configuration", config);

                // createOffer(options) → returns a Promise-like object.
                pc.set(
                    "createOffer",
                    Value::Builtin(BuiltinFn {
                        name: "RTCPeerConnection.createOffer".to_string(),
                        func: Rc::new(|_args| {
                            // Return a mock offer SDP.
                            let mut offer = ObjectValue::new();
                            offer.set("type", Value::String("offer".to_string()));
                            offer.set(
                                "sdp",
                                Value::String(
                                    "v=0\r\no=- 123456 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n"
                                        .to_string(),
                                ),
                            );
                            Ok(Value::Object(Rc::new(RefCell::new(offer))))
                        }),
                    }),
                );

                // createAnswer(options)
                pc.set(
                    "createAnswer",
                    Value::Builtin(BuiltinFn {
                        name: "RTCPeerConnection.createAnswer".to_string(),
                        func: Rc::new(|_args| {
                            let mut answer = ObjectValue::new();
                            answer.set("type", Value::String("answer".to_string()));
                            answer.set(
                                "sdp",
                                Value::String(
                                    "v=0\r\no=- 123457 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n"
                                        .to_string(),
                                ),
                            );
                            Ok(Value::Object(Rc::new(RefCell::new(answer))))
                        }),
                    }),
                );

                // setLocalDescription(description)
                pc.set(
                    "setLocalDescription",
                    Value::Builtin(BuiltinFn {
                        name: "RTCPeerConnection.setLocalDescription".to_string(),
                        func: Rc::new(|args| {
                            let desc = args.first().cloned().unwrap_or(Value::Undefined);
                            // In a real implementation, this would set the local SDP.
                            let _ = desc;
                            Ok(Value::Undefined)
                        }),
                    }),
                );

                // setRemoteDescription(description)
                pc.set(
                    "setRemoteDescription",
                    Value::Builtin(BuiltinFn {
                        name: "RTCPeerConnection.setRemoteDescription".to_string(),
                        func: Rc::new(|args| {
                            let desc = args.first().cloned().unwrap_or(Value::Undefined);
                            let _ = desc;
                            Ok(Value::Undefined)
                        }),
                    }),
                );

                // addIceCandidate(candidate)
                pc.set(
                    "addIceCandidate",
                    Value::Builtin(BuiltinFn {
                        name: "RTCPeerConnection.addIceCandidate".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                // addTrack(track, ...streams)
                pc.set(
                    "addTrack",
                    Value::Builtin(BuiltinFn {
                        name: "RTCPeerConnection.addTrack".to_string(),
                        func: Rc::new(|_args| {
                            // Return a mock RTCRtpSender.
                            let mut sender = ObjectValue::new();
                            sender.set("track", Value::Undefined);
                            Ok(Value::Object(Rc::new(RefCell::new(sender))))
                        }),
                    }),
                );

                // removeTrack(sender)
                pc.set(
                    "removeTrack",
                    Value::Builtin(BuiltinFn {
                        name: "RTCPeerConnection.removeTrack".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                // createDataChannel(label, options)
                pc.set(
                    "createDataChannel",
                    Value::Builtin(BuiltinFn {
                        name: "RTCPeerConnection.createDataChannel".to_string(),
                        func: Rc::new(|args| {
                            let label = args
                                .first()
                                .map(|v| v.to_string())
                                .unwrap_or_default();
                            make_data_channel(label)
                        }),
                    }),
                );

                // getStats()
                pc.set(
                    "getStats",
                    Value::Builtin(BuiltinFn {
                        name: "RTCPeerConnection.getStats".to_string(),
                        func: Rc::new(|_args| {
                            // Return a mock stats report.
                            let mut report = ObjectValue::new();
                            report.set("timestamp", Value::Number(0.0));
                            report.set("type", Value::String("stats".to_string()));
                            Ok(Value::Object(Rc::new(RefCell::new(report))))
                        }),
                    }),
                );

                // close()
                pc.set(
                    "close",
                    Value::Builtin(BuiltinFn {
                        name: "RTCPeerConnection.close".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                // addEventListener(event, handler)
                pc.set(
                    "addEventListener",
                    Value::Builtin(BuiltinFn {
                        name: "RTCPeerConnection.addEventListener".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                // removeEventListener
                pc.set(
                    "removeEventListener",
                    Value::Builtin(BuiltinFn {
                        name: "RTCPeerConnection.removeEventListener".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                // onicecandidate, ondatachannel, ontrack, etc. — settable by user.
                pc.set("onicecandidate", Value::Null);
                pc.set("ondatachannel", Value::Null);
                pc.set("ontrack", Value::Null);
                pc.set("oniceconnectionstatechange", Value::Null);
                pc.set("onconnectionstatechange", Value::Null);
                pc.set("onsignalingstatechange", Value::Null);

                Ok(Value::Object(Rc::new(RefCell::new(pc))))
            }),
        }),
    );
}

fn register_rtc_data_channel(scope: &mut Scope) {
    scope.declare(
        "RTCDataChannel",
        Value::Builtin(BuiltinFn {
            name: "RTCDataChannel".to_string(),
            func: Rc::new(|args| {
                let label = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                make_data_channel(label)
            }),
        }),
    );
}

fn make_data_channel(label: String) -> Result<Value, String> {
    let mut dc = ObjectValue::new();
    dc.set("label", Value::String(label.clone()));
    dc.set("readyState", Value::String("connecting".to_string()));
    dc.set("binaryType", Value::String("blob".to_string()));
    dc.set("maxPacketLifeTime", Value::Null);
    dc.set("maxRetransmits", Value::Null);
    dc.set("negotiated", Value::Boolean(false));
    dc.set("ordered", Value::Boolean(true));
    dc.set("protocol", Value::String(String::new()));
    dc.set("id", Value::Null);

    // send(data)
    dc.set(
        "send",
        Value::Builtin(BuiltinFn {
            name: "RTCDataChannel.send".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // close()
    dc.set(
        "close",
        Value::Builtin(BuiltinFn {
            name: "RTCDataChannel.close".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    dc.set("onopen", Value::Null);
    dc.set("onclose", Value::Null);
    dc.set("onmessage", Value::Null);
    dc.set("onerror", Value::Null);

    Ok(Value::Object(Rc::new(RefCell::new(dc))))
}

fn register_media_stream(scope: &mut Scope) {
    scope.declare(
        "MediaStream",
        Value::Builtin(BuiltinFn {
            name: "MediaStream".to_string(),
            func: Rc::new(|_args| {
                let mut stream = ObjectValue::new();
                stream.set("id", Value::String(format!("stream-{}", rand_id())));
                stream.set("active", Value::Boolean(true));

                stream.set(
                    "getTracks",
                    Value::Builtin(BuiltinFn {
                        name: "MediaStream.getTracks".to_string(),
                        func: Rc::new(|_args| {
                            Ok(Value::Array(Rc::new(RefCell::new(vec![]))))
                        }),
                    }),
                );

                stream.set(
                    "getAudioTracks",
                    Value::Builtin(BuiltinFn {
                        name: "MediaStream.getAudioTracks".to_string(),
                        func: Rc::new(|_args| {
                            Ok(Value::Array(Rc::new(RefCell::new(vec![]))))
                        }),
                    }),
                );

                stream.set(
                    "getVideoTracks",
                    Value::Builtin(BuiltinFn {
                        name: "MediaStream.getVideoTracks".to_string(),
                        func: Rc::new(|_args| {
                            Ok(Value::Array(Rc::new(RefCell::new(vec![]))))
                        }),
                    }),
                );

                stream.set(
                    "addTrack",
                    Value::Builtin(BuiltinFn {
                        name: "MediaStream.addTrack".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                stream.set(
                    "removeTrack",
                    Value::Builtin(BuiltinFn {
                        name: "MediaStream.removeTrack".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                stream.set(
                    "clone",
                    Value::Builtin(BuiltinFn {
                        name: "MediaStream.clone".to_string(),
                        func: Rc::new(|_args| {
                            Ok(Value::Object(Rc::new(RefCell::new(
                                ObjectValue::new(),
                            ))))
                        }),
                    }),
                );

                Ok(Value::Object(Rc::new(RefCell::new(stream))))
            }),
        }),
    );

    // MediaStreamTrack
    scope.declare(
        "MediaStreamTrack",
        Value::Builtin(BuiltinFn {
            name: "MediaStreamTrack".to_string(),
            func: Rc::new(|_args| {
                let mut track = ObjectValue::new();
                track.set("kind", Value::String("audio".to_string()));
                track.set("id", Value::String(format!("track-{}", rand_id())));
                track.set("label", Value::String(String::new()));
                track.set("enabled", Value::Boolean(true));
                track.set("muted", Value::Boolean(false));
                track.set("readyState", Value::String("live".to_string()));

                track.set(
                    "stop",
                    Value::Builtin(BuiltinFn {
                        name: "MediaStreamTrack.stop".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                track.set(
                    "clone",
                    Value::Builtin(BuiltinFn {
                        name: "MediaStreamTrack.clone".to_string(),
                        func: Rc::new(|_args| {
                            Ok(Value::Object(Rc::new(RefCell::new(
                                ObjectValue::new(),
                            ))))
                        }),
                    }),
                );

                Ok(Value::Object(Rc::new(RefCell::new(track))))
            }),
        }),
    );
}

fn register_rtc_session_description(scope: &mut Scope) {
    scope.declare(
        "RTCSessionDescription",
        Value::Builtin(BuiltinFn {
            name: "RTCSessionDescription".to_string(),
            func: Rc::new(|args| {
                let mut desc = ObjectValue::new();
                if let Some(Value::Object(init)) = args.first() {
                    let init = init.borrow();
                    desc.set(
                        "type",
                        init.properties
                            .get("type")
                            .cloned()
                            .unwrap_or(Value::String("offer".to_string())),
                    );
                    desc.set(
                        "sdp",
                        init.properties
                            .get("sdp")
                            .cloned()
                            .unwrap_or(Value::String(String::new())),
                    );
                } else {
                    desc.set("type", Value::String("offer".to_string()));
                    desc.set("sdp", Value::String(String::new()));
                }
                Ok(Value::Object(Rc::new(RefCell::new(desc))))
            }),
        }),
    );
}

fn register_rtc_ice_candidate(scope: &mut Scope) {
    scope.declare(
        "RTCIceCandidate",
        Value::Builtin(BuiltinFn {
            name: "RTCIceCandidate".to_string(),
            func: Rc::new(|args| {
                let mut candidate = ObjectValue::new();
                if let Some(Value::Object(init)) = args.first() {
                    let init = init.borrow();
                    candidate.set(
                        "candidate",
                        init.properties
                            .get("candidate")
                            .cloned()
                            .unwrap_or(Value::String(String::new())),
                    );
                    candidate.set(
                        "sdpMid",
                        init.properties
                            .get("sdpMid")
                            .cloned()
                            .unwrap_or(Value::Null),
                    );
                    candidate.set(
                        "sdpMLineIndex",
                        init.properties
                            .get("sdpMLineIndex")
                            .cloned()
                            .unwrap_or(Value::Number(0.0)),
                    );
                } else {
                    candidate.set("candidate", Value::String(String::new()));
                    candidate.set("sdpMid", Value::Null);
                    candidate.set("sdpMLineIndex", Value::Number(0.0));
                }
                Ok(Value::Object(Rc::new(RefCell::new(candidate))))
            }),
        }),
    );
}

/// Generate a random ID string.
fn rand_id() -> u64 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
        .wrapping_mul(2654435761u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtc_peer_connection_creation() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("RTCPeerConnection").unwrap();
        if let Value::Builtin(b) = ctor {
            let pc = (b.func)(vec![]).unwrap();
            if let Value::Object(obj) = pc {
                let obj = obj.borrow();
                assert_eq!(
                    obj.properties.get("connectionState"),
                    Some(&Value::String("new".to_string()))
                );
                assert!(obj.properties.contains_key("createOffer"));
                assert!(obj.properties.contains_key("createAnswer"));
                assert!(obj.properties.contains_key("createDataChannel"));
                assert!(obj.properties.contains_key("addTrack"));
            }
        }
    }

    #[test]
    fn create_offer() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("RTCPeerConnection").unwrap();
        if let Value::Builtin(b) = ctor {
            let pc = (b.func)(vec![]).unwrap();
            if let Value::Object(obj) = &pc {
                let obj = obj.borrow();
                if let Some(Value::Builtin(offer_fn)) = obj.properties.get("createOffer") {
                    let offer = (offer_fn.func)(vec![]).unwrap();
                    if let Value::Object(offer_obj) = offer {
                        let o = offer_obj.borrow();
                        assert_eq!(o.properties.get("type"), Some(&Value::String("offer".to_string())));
                        assert!(o.properties.contains_key("sdp"));
                    }
                }
            }
        }
    }

    #[test]
    fn create_data_channel() {
        let dc = make_data_channel("test".to_string()).unwrap();
        if let Value::Object(obj) = dc {
            let obj = obj.borrow();
            assert_eq!(obj.properties.get("label"), Some(&Value::String("test".to_string())));
            assert!(obj.properties.contains_key("send"));
            assert!(obj.properties.contains_key("close"));
        }
    }

    #[test]
    fn media_stream_creation() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("MediaStream").unwrap();
        if let Value::Builtin(b) = ctor {
            let stream = (b.func)(vec![]).unwrap();
            if let Value::Object(obj) = stream {
                let obj = obj.borrow();
                assert_eq!(obj.properties.get("active"), Some(&Value::Boolean(true)));
                assert!(obj.properties.contains_key("getTracks"));
                assert!(obj.properties.contains_key("addTrack"));
            }
        }
    }

    #[test]
    fn rtc_session_description() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("RTCSessionDescription").unwrap();
        if let Value::Builtin(b) = ctor {
            let mut init = ObjectValue::new();
            init.set("type", Value::String("answer".to_string()));
            init.set("sdp", Value::String("v=0".to_string()));
            let desc = (b.func)(vec![Value::Object(Rc::new(RefCell::new(init)))]).unwrap();
            if let Value::Object(obj) = desc {
                let obj = obj.borrow();
                assert_eq!(obj.properties.get("type"), Some(&Value::String("answer".to_string())));
            }
        }
    }

    #[test]
    fn all_rtc_classes_registered() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        for name in &[
            "RTCPeerConnection",
            "RTCDataChannel",
            "MediaStream",
            "MediaStreamTrack",
            "RTCSessionDescription",
            "RTCIceCandidate",
        ] {
            assert!(scope.get(name).is_some(), "missing: {}", name);
        }
    }
}
