//! WebGPU API — modern GPU access for graphics and compute.
//!
//! # Overview
//!
//! WebGPU is the successor to WebGL, providing:
//! - Lower overhead (explicit GPU command recording)
//! - Compute shaders (general-purpose GPU compute)
//! - Better multi-threading support
//! - Modern GPU features (bind groups, render bundles)
//!
//! ```js
//! const adapter = await navigator.gpu.requestAdapter();
//! const device = await adapter.requestDevice();
//! const shader = device.createShaderModule({ code: '...' });
//! const pipeline = device.createRenderPipeline({ ... });
//! ```
//!
//! # Implementation
//!
//! Real WebGPU requires a native GPU backend (Vulkan, Metal, D3D12).
//! Falco implements the **API surface** with stub implementations —
//! no actual GPU commands are executed. This allows testing code that
//! uses the WebGPU API without requiring a GPU.

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Register the WebGPU API.
pub fn register(scope: &mut Scope) {
    // navigator.gpu
    let mut gpu_obj = ObjectValue::new();

    // requestAdapter(options)
    gpu_obj.set(
        "requestAdapter",
        Value::Builtin(BuiltinFn {
            name: "GPU.requestAdapter".to_string(),
            func: Rc::new(|_args| make_adapter()),
        }),
    );

    // wgslLanguageFeatures
    gpu_obj.set(
        "wgslLanguageFeatures",
        Value::Object(Rc::new(RefCell::new(ObjectValue::new()))),
    );

    // Add to navigator.
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

    nav.set("gpu", Value::Object(Rc::new(RefCell::new(gpu_obj))));
    scope.declare("navigator", Value::Object(Rc::new(RefCell::new(nav))));

    // GPU global constants.
    let mut gpu_constants = ObjectValue::new();
    for (name, val) in &[
        ("MAX_TEXTURE_DIMENSION_2D", 8192.0),
        ("MAX_TEXTURE_DIMENSION_3D", 2048.0),
        ("MAX_TEXTURE_ARRAY_LAYERS", 256.0),
        ("MAX_BIND_GROUPS", 4.0),
        ("MAX_VERTEX_BUFFERS", 8.0),
        ("MAX_VERTEX_ATTRIBUTES", 16.0),
        ("MAX_SAMPLED_TEXTURES_PER_SHADER_STAGE", 16.0),
        ("MAX_STORAGE_BUFFERS_PER_SHADER_STAGE", 8.0),
    ] {
        gpu_constants.set(name, Value::Number(*val));
    }
    scope.declare("GPUConstants", Value::Object(Rc::new(RefCell::new(gpu_constants))));
}

/// Create a mock GPUAdapter.
fn make_adapter() -> Result<Value, String> {
    let mut adapter = ObjectValue::new();

    // adapter info
    let mut info = ObjectValue::new();
    info.set("vendor", Value::String("Falco".to_string()));
    info.set("architecture", Value::String("Mock GPU".to_string()));
    info.set("device", Value::String("Falco WebGPU".to_string()));
    info.set("description", Value::String("Software WebGPU implementation".to_string()));
    adapter.set("info", Value::Object(Rc::new(RefCell::new(info))));

    // adapter features (empty set).
    adapter.set("features", Value::Object(Rc::new(RefCell::new(ObjectValue::new()))));

    // adapter limits.
    let mut limits = ObjectValue::new();
    limits.set("maxTextureDimension2D", Value::Number(8192.0));
    limits.set("maxTextureDimension3D", Value::Number(2048.0));
    limits.set("maxTextureArrayLayers", Value::Number(256.0));
    limits.set("maxBindGroups", Value::Number(4.0));
    limits.set("maxVertexBuffers", Value::Number(8.0));
    limits.set("maxVertexAttributes", Value::Number(16.0));
    limits.set("maxBufferSize", Value::Number(1_073_741_824.0)); // 1 GB
    limits.set("maxStorageBufferBindingSize", Value::Number(134_217_728.0)); // 128 MB
    adapter.set("limits", Value::Object(Rc::new(RefCell::new(limits))));

    adapter.set("isFallbackAdapter", Value::Boolean(true));

    // requestDevice(descriptor)
    adapter.set(
        "requestDevice",
        Value::Builtin(BuiltinFn {
            name: "GPUAdapter.requestDevice".to_string(),
            func: Rc::new(|_args| make_device()),
        }),
    );

    // requestAdapterInfo()
    adapter.set(
        "requestAdapterInfo",
        Value::Builtin(BuiltinFn {
            name: "GPUAdapter.requestAdapterInfo".to_string(),
            func: Rc::new(|_args| {
                let mut info = ObjectValue::new();
                info.set("vendor", Value::String("Falco".to_string()));
                info.set("architecture", Value::String("Mock GPU".to_string()));
                info.set("device", Value::String("Falco WebGPU".to_string()));
                info.set("description", Value::String("Software WebGPU".to_string()));
                Ok(Value::Object(Rc::new(RefCell::new(info))))
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(adapter))))
}

/// Create a mock GPUDevice.
fn make_device() -> Result<Value, String> {
    let mut device = ObjectValue::new();

    device.set("lost", Value::Undefined); // Would be a Promise.
    device.set("queue", make_queue()?);

    // device features
    device.set("features", Value::Object(Rc::new(RefCell::new(ObjectValue::new()))));

    // device limits
    let mut limits = ObjectValue::new();
    limits.set("maxTextureDimension2D", Value::Number(8192.0));
    limits.set("maxTextureDimension3D", Value::Number(2048.0));
    limits.set("maxTextureArrayLayers", Value::Number(256.0));
    limits.set("maxBindGroups", Value::Number(4.0));
    limits.set("maxVertexBuffers", Value::Number(8.0));
    limits.set("maxVertexAttributes", Value::Number(16.0));
    device.set("limits", Value::Object(Rc::new(RefCell::new(limits))));

    // createBuffer(descriptor)
    device.set(
        "createBuffer",
        Value::Builtin(BuiltinFn {
            name: "GPUDevice.createBuffer".to_string(),
            func: Rc::new(|args| {
                let size = if let Some(Value::Object(desc)) = args.first() {
                    desc.borrow().properties.get("size").map(|v| v.to_number()).unwrap_or(0.0)
                } else {
                    0.0
                };
                let mut buffer = ObjectValue::new();
                buffer.set("size", Value::Number(size));
                buffer.set("usage", Value::Number(0.0));
                buffer.set("mapState", Value::String("unmapped".to_string()));
                buffer.set(
                    "getMappedRange",
                    Value::Builtin(BuiltinFn {
                        name: "GPUBuffer.getMappedRange".to_string(),
                        func: Rc::new(|_args| Ok(Value::Array(Rc::new(RefCell::new(vec![]))))),
                    }),
                );
                buffer.set(
                    "mapAsync",
                    Value::Builtin(BuiltinFn {
                        name: "GPUBuffer.mapAsync".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                buffer.set(
                    "unmap",
                    Value::Builtin(BuiltinFn {
                        name: "GPUBuffer.unmap".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                buffer.set(
                    "destroy",
                    Value::Builtin(BuiltinFn {
                        name: "GPUBuffer.destroy".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                Ok(Value::Object(Rc::new(RefCell::new(buffer))))
            }),
        }),
    );

    // createTexture(descriptor)
    device.set(
        "createTexture",
        Value::Builtin(BuiltinFn {
            name: "GPUDevice.createTexture".to_string(),
            func: Rc::new(|_args| {
                let mut texture = ObjectValue::new();
                texture.set("width", Value::Number(0.0));
                texture.set("height", Value::Number(0.0));
                texture.set("format", Value::String("rgba8unorm".to_string()));
                texture.set(
                    "createView",
                    Value::Builtin(BuiltinFn {
                        name: "GPUTexture.createView".to_string(),
                        func: Rc::new(|_args| Ok(Value::Object(Rc::new(RefCell::new(ObjectValue::new()))))),
                    }),
                );
                texture.set(
                    "destroy",
                    Value::Builtin(BuiltinFn {
                        name: "GPUTexture.destroy".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                Ok(Value::Object(Rc::new(RefCell::new(texture))))
            }),
        }),
    );

    // createSampler(descriptor)
    device.set(
        "createSampler",
        Value::Builtin(BuiltinFn {
            name: "GPUDevice.createSampler".to_string(),
            func: Rc::new(|_args| Ok(Value::Object(Rc::new(RefCell::new(ObjectValue::new()))))),
        }),
    );

    // createBindGroupLayout(descriptor)
    device.set(
        "createBindGroupLayout",
        Value::Builtin(BuiltinFn {
            name: "GPUDevice.createBindGroupLayout".to_string(),
            func: Rc::new(|_args| Ok(Value::Object(Rc::new(RefCell::new(ObjectValue::new()))))),
        }),
    );

    // createBindGroup(descriptor)
    device.set(
        "createBindGroup",
        Value::Builtin(BuiltinFn {
            name: "GPUDevice.createBindGroup".to_string(),
            func: Rc::new(|_args| Ok(Value::Object(Rc::new(RefCell::new(ObjectValue::new()))))),
        }),
    );

    // createPipelineLayout(descriptor)
    device.set(
        "createPipelineLayout",
        Value::Builtin(BuiltinFn {
            name: "GPUDevice.createPipelineLayout".to_string(),
            func: Rc::new(|_args| Ok(Value::Object(Rc::new(RefCell::new(ObjectValue::new()))))),
        }),
    );

    // createShaderModule(descriptor)
    device.set(
        "createShaderModule",
        Value::Builtin(BuiltinFn {
            name: "GPUDevice.createShaderModule".to_string(),
            func: Rc::new(|args| {
                let code = if let Some(Value::Object(desc)) = args.first() {
                    desc.borrow().properties.get("code").map(|v| v.to_string()).unwrap_or_default()
                } else {
                    String::new()
                };
                let mut module = ObjectValue::new();
                module.set("__code", Value::String(code));
                module.set(
                    "getCompilationInfo",
                    Value::Builtin(BuiltinFn {
                        name: "GPUShaderModule.getCompilationInfo".to_string(),
                        func: Rc::new(|_args| {
                            let mut info = ObjectValue::new();
                            info.set("messages", Value::Array(Rc::new(RefCell::new(vec![]))));
                            Ok(Value::Object(Rc::new(RefCell::new(info))))
                        }),
                    }),
                );
                Ok(Value::Object(Rc::new(RefCell::new(module))))
            }),
        }),
    );

    // createComputePipeline(descriptor)
    device.set(
        "createComputePipeline",
        Value::Builtin(BuiltinFn {
            name: "GPUDevice.createComputePipeline".to_string(),
            func: Rc::new(|_args| {
                let mut pipeline = ObjectValue::new();
                pipeline.set(
                    "getBindGroupLayout",
                    Value::Builtin(BuiltinFn {
                        name: "GPUComputePipeline.getBindGroupLayout".to_string(),
                        func: Rc::new(|_args| Ok(Value::Object(Rc::new(RefCell::new(ObjectValue::new()))))),
                    }),
                );
                Ok(Value::Object(Rc::new(RefCell::new(pipeline))))
            }),
        }),
    );

    // createRenderPipeline(descriptor)
    device.set(
        "createRenderPipeline",
        Value::Builtin(BuiltinFn {
            name: "GPUDevice.createRenderPipeline".to_string(),
            func: Rc::new(|_args| {
                let mut pipeline = ObjectValue::new();
                pipeline.set(
                    "getBindGroupLayout",
                    Value::Builtin(BuiltinFn {
                        name: "GPURenderPipeline.getBindGroupLayout".to_string(),
                        func: Rc::new(|_args| Ok(Value::Object(Rc::new(RefCell::new(ObjectValue::new()))))),
                    }),
                );
                Ok(Value::Object(Rc::new(RefCell::new(pipeline))))
            }),
        }),
    );

    // createComputePipelineAsync / createRenderPipelineAsync
    device.set(
        "createComputePipelineAsync",
        Value::Builtin(BuiltinFn {
            name: "GPUDevice.createComputePipelineAsync".to_string(),
            func: Rc::new(|_args| Ok(Value::Object(Rc::new(RefCell::new(ObjectValue::new()))))),
        }),
    );

    device.set(
        "createRenderPipelineAsync",
        Value::Builtin(BuiltinFn {
            name: "GPUDevice.createRenderPipelineAsync".to_string(),
            func: Rc::new(|_args| Ok(Value::Object(Rc::new(RefCell::new(ObjectValue::new()))))),
        }),
    );

    // createCommandEncoder(descriptor)
    device.set(
        "createCommandEncoder",
        Value::Builtin(BuiltinFn {
            name: "GPUDevice.createCommandEncoder".to_string(),
            func: Rc::new(|_args| {
                let mut encoder = ObjectValue::new();
                encoder.set(
                    "beginRenderPass",
                    Value::Builtin(BuiltinFn {
                        name: "GPUCommandEncoder.beginRenderPass".to_string(),
                        func: Rc::new(|_args| make_render_pass_encoder()),
                    }),
                );
                encoder.set(
                    "beginComputePass",
                    Value::Builtin(BuiltinFn {
                        name: "GPUCommandEncoder.beginComputePass".to_string(),
                        func: Rc::new(|_args| make_compute_pass_encoder()),
                    }),
                );
                encoder.set(
                    "copyBufferToBuffer",
                    Value::Builtin(BuiltinFn {
                        name: "GPUCommandEncoder.copyBufferToBuffer".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                encoder.set(
                    "copyBufferToTexture",
                    Value::Builtin(BuiltinFn {
                        name: "GPUCommandEncoder.copyBufferToTexture".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                encoder.set(
                    "copyTextureToBuffer",
                    Value::Builtin(BuiltinFn {
                        name: "GPUCommandEncoder.copyTextureToBuffer".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                encoder.set(
                    "copyTextureToTexture",
                    Value::Builtin(BuiltinFn {
                        name: "GPUCommandEncoder.copyTextureToTexture".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                encoder.set(
                    "clearBuffer",
                    Value::Builtin(BuiltinFn {
                        name: "GPUCommandEncoder.clearBuffer".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                encoder.set(
                    "finish",
                    Value::Builtin(BuiltinFn {
                        name: "GPUCommandEncoder.finish".to_string(),
                        func: Rc::new(|_args| Ok(Value::Object(Rc::new(RefCell::new(ObjectValue::new()))))),
                    }),
                );
                Ok(Value::Object(Rc::new(RefCell::new(encoder))))
            }),
        }),
    );

    // createRenderBundleEncoder(descriptor)
    device.set(
        "createRenderBundleEncoder",
        Value::Builtin(BuiltinFn {
            name: "GPUDevice.createRenderBundleEncoder".to_string(),
            func: Rc::new(|_args| Ok(Value::Object(Rc::new(RefCell::new(ObjectValue::new()))))),
        }),
    );

    // createQuerySet(descriptor)
    device.set(
        "createQuerySet",
        Value::Builtin(BuiltinFn {
            name: "GPUDevice.createQuerySet".to_string(),
            func: Rc::new(|_args| Ok(Value::Object(Rc::new(RefCell::new(ObjectValue::new()))))),
        }),
    );

    // pushErrorScope / popErrorScope
    device.set(
        "pushErrorScope",
        Value::Builtin(BuiltinFn {
            name: "GPUDevice.pushErrorScope".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    device.set(
        "popErrorScope",
        Value::Builtin(BuiltinFn {
            name: "GPUDevice.popErrorScope".to_string(),
            func: Rc::new(|_args| Ok(Value::Null)),
        }),
    );

    // onuncapturederror
    device.set("onuncapturederror", Value::Null);

    // destroy()
    device.set(
        "destroy",
        Value::Builtin(BuiltinFn {
            name: "GPUDevice.destroy".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(device))))
}

/// Create a mock GPUQueue.
fn make_queue() -> Result<Value, String> {
    let mut queue = ObjectValue::new();

    // submit(commandBuffers)
    queue.set(
        "submit",
        Value::Builtin(BuiltinFn {
            name: "GPUQueue.submit".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // writeBuffer(buffer, bufferOffset, data)
    queue.set(
        "writeBuffer",
        Value::Builtin(BuiltinFn {
            name: "GPUQueue.writeBuffer".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // writeTexture(destination, data, dataSize, destinationSize)
    queue.set(
        "writeTexture",
        Value::Builtin(BuiltinFn {
            name: "GPUQueue.writeTexture".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // copyExternalImageToTexture(source, destination, copySize)
    queue.set(
        "copyExternalImageToTexture",
        Value::Builtin(BuiltinFn {
            name: "GPUQueue.copyExternalImageToTexture".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // onSubmittedWorkDone()
    queue.set(
        "onSubmittedWorkDone",
        Value::Builtin(BuiltinFn {
            name: "GPUQueue.onSubmittedWorkDone".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(queue))))
}

/// Create a mock GPURenderPassEncoder.
fn make_render_pass_encoder() -> Result<Value, String> {
    let mut encoder = ObjectValue::new();

    for method in &[
        "setPipeline",
        "setBindGroup",
        "setVertexBuffer",
        "setIndexBuffer",
        "setViewport",
        "setScissorRect",
        "setBlendConstant",
        "setStencilReference",
        "draw",
        "drawIndexed",
        "drawIndirect",
        "drawIndexedIndirect",
        "executeBundles",
        "beginOcclusionQuery",
        "endOcclusionQuery",
    ] {
        let m = method.to_string();
        encoder.set(
            method,
            Value::Builtin(BuiltinFn {
                name: format!("GPURenderPassEncoder.{}", m),
                func: Rc::new(|_args| Ok(Value::Undefined)),
            }),
        );
    }

    encoder.set(
        "end",
        Value::Builtin(BuiltinFn {
            name: "GPURenderPassEncoder.end".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(encoder))))
}

/// Create a mock GPUComputePassEncoder.
fn make_compute_pass_encoder() -> Result<Value, String> {
    let mut encoder = ObjectValue::new();

    for method in &["setPipeline", "setBindGroup", "dispatchWorkgroups", "dispatchWorkgroupsIndirect"] {
        let m = method.to_string();
        encoder.set(
            method,
            Value::Builtin(BuiltinFn {
                name: format!("GPUComputePassEncoder.{}", m),
                func: Rc::new(|_args| Ok(Value::Undefined)),
            }),
        );
    }

    encoder.set(
        "end",
        Value::Builtin(BuiltinFn {
            name: "GPUComputePassEncoder.end".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(encoder))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_navigator_exists() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            assert!(nav_obj.borrow().properties.contains_key("gpu"));
        }
    }

    #[test]
    fn request_adapter() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            let nav_obj = nav_obj.borrow();
            if let Some(Value::Object(gpu_obj)) = nav_obj.properties.get("gpu") {
                let gpu_obj = gpu_obj.borrow();
                if let Some(Value::Builtin(req_fn)) = gpu_obj.properties.get("requestAdapter") {
                    let adapter = (req_fn.func)(vec![]).unwrap();
                    if let Value::Object(a) = adapter {
                        let a = a.borrow();
                        assert!(a.properties.contains_key("requestDevice"));
                        assert!(a.properties.contains_key("features"));
                        assert!(a.properties.contains_key("limits"));
                    }
                }
            }
        }
    }

    #[test]
    fn request_device() {
        let adapter = make_adapter().unwrap();
        if let Value::Object(obj) = &adapter {
            let obj = obj.borrow();
            if let Some(Value::Builtin(req_fn)) = obj.properties.get("requestDevice") {
                let device = (req_fn.func)(vec![]).unwrap();
                if let Value::Object(d) = device {
                    let d = d.borrow();
                    assert!(d.properties.contains_key("createBuffer"));
                    assert!(d.properties.contains_key("createTexture"));
                    assert!(d.properties.contains_key("createShaderModule"));
                    assert!(d.properties.contains_key("createRenderPipeline"));
                    assert!(d.properties.contains_key("createCommandEncoder"));
                    assert!(d.properties.contains_key("queue"));
                }
            }
        }
    }

    #[test]
    fn create_buffer() {
        let device = make_device().unwrap();
        if let Value::Object(obj) = &device {
            let obj = obj.borrow();
            if let Some(Value::Builtin(create_fn)) = obj.properties.get("createBuffer") {
                let mut desc = ObjectValue::new();
                desc.set("size", Value::Number(1024.0));
                let buffer = (create_fn.func)(vec![Value::Object(Rc::new(RefCell::new(desc)))]).unwrap();
                if let Value::Object(b) = buffer {
                    let b = b.borrow();
                    assert_eq!(b.properties.get("size"), Some(&Value::Number(1024.0)));
                    assert!(b.properties.contains_key("mapAsync"));
                    assert!(b.properties.contains_key("unmap"));
                    assert!(b.properties.contains_key("destroy"));
                }
            }
        }
    }

    #[test]
    fn create_shader_module() {
        let device = make_device().unwrap();
        if let Value::Object(obj) = &device {
            let obj = obj.borrow();
            if let Some(Value::Builtin(create_fn)) = obj.properties.get("createShaderModule") {
                let mut desc = ObjectValue::new();
                desc.set("code", Value::String("@vertex fn vs() {}".to_string()));
                let module = (create_fn.func)(vec![Value::Object(Rc::new(RefCell::new(desc)))]).unwrap();
                if let Value::Object(m) = module {
                    assert!(m.borrow().properties.contains_key("getCompilationInfo"));
                }
            }
        }
    }

    #[test]
    fn command_encoder_methods() {
        let device = make_device().unwrap();
        if let Value::Object(obj) = &device {
            let obj = obj.borrow();
            if let Some(Value::Builtin(create_fn)) = obj.properties.get("createCommandEncoder") {
                let encoder = (create_fn.func)(vec![]).unwrap();
                if let Value::Object(e) = encoder {
                    let e = e.borrow();
                    assert!(e.properties.contains_key("beginRenderPass"));
                    assert!(e.properties.contains_key("beginComputePass"));
                    assert!(e.properties.contains_key("copyBufferToBuffer"));
                    assert!(e.properties.contains_key("finish"));
                }
            }
        }
    }

    #[test]
    fn gpu_constants() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let constants = scope.get("GPUConstants").unwrap();
        if let Value::Object(obj) = constants {
            let obj = obj.borrow();
            assert_eq!(obj.properties.get("MAX_TEXTURE_DIMENSION_2D"), Some(&Value::Number(8192.0)));
            assert_eq!(obj.properties.get("MAX_BIND_GROUPS"), Some(&Value::Number(4.0)));
        }
    }
}
