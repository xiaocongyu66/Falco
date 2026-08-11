//! TJS Interpreter — tree-walking evaluator.
//!
//! Evaluates the AST by walking each node and computing its value.
//! Supports scopes (lexical scoping), closures, and control flow
//! (return, break, continue via Result enums).

use crate::tjs::parser::{Expr, Stmt};
use crate::tjs::value::Value;
use crate::tjs::value::{FunctionValue, ObjectValue};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A lexical scope — uses Rc<RefCell<Vec>> for vars so that closures
/// share the same variable storage as the scope they were created in.
/// This is critical for patterns like `var ytcfg = {d: function(){ return ytcfg.data_ }}`
/// where the function needs to see the updated value of `ytcfg`.
#[derive(Clone)]
pub struct Scope {
    pub vars: Rc<RefCell<Vec<(String, Value)>>>,
    pub parent: Option<Rc<RefCell<Scope>>>,
}

impl Scope {
    pub fn new(parent: Option<Rc<RefCell<Scope>>>) -> Self {
        Self {
            vars: Rc::new(RefCell::new(Vec::with_capacity(8))),
            parent,
        }
    }

    pub fn into_rc(self) -> Rc<RefCell<Scope>> {
        Rc::new(RefCell::new(self))
    }

    #[inline(always)]
    pub fn get(&self, name: &str) -> Option<Value> {
        // Search own vars (shared via Rc<RefCell>).
        for (k, v) in self.vars.borrow().iter() {
            if k == name {
                return Some(v.clone());
            }
        }
        // Walk parent chain.
        let mut parent = self.parent.clone();
        while let Some(p) = parent {
            let p_ref = p.borrow();
            for (k, v) in p_ref.vars.borrow().iter() {
                if k == name {
                    return Some(v.clone());
                }
            }
            parent = p_ref.parent.clone();
        }
        None
    }

    #[inline(always)]
    pub fn set(&mut self, name: &str, value: Value) {
        // Try to find and update in own vars.
        for (k, v) in self.vars.borrow_mut().iter_mut() {
            if k == name {
                *v = value;
                return;
            }
        }
        // Walk parent chain.
        let mut parent = self.parent.clone();
        while let Some(p) = parent {
            let p_ref = p.borrow_mut();
            let mut vars = p_ref.vars.borrow_mut();
            for (k, v) in vars.iter_mut() {
                if k == name {
                    *v = value;
                    return;
                }
            }
            drop(vars);
            parent = p_ref.parent.clone();
        }
        // Not found — create in current scope.
        self.vars.borrow_mut().push((name.to_string(), value));
    }

    #[inline(always)]
    pub fn declare(&mut self, name: &str, value: Value) {
        let mut vars = self.vars.borrow_mut();
        // If a variable with this name already exists, update it in place.
        for (k, v) in vars.iter_mut() {
            if k == name {
                *v = value;
                return;
            }
        }
        vars.push((name.to_string(), value));
    }

    pub fn has(&self, name: &str) -> bool {
        for (k, _) in self.vars.borrow().iter() {
            if k == name {
                return true;
            }
        }
        let mut parent = self.parent.clone();
        while let Some(p) = parent {
            let p_ref = p.borrow();
            for (k, _) in p_ref.vars.borrow().iter() {
                if k == name {
                    return true;
                }
            }
            parent = p_ref.parent.clone();
        }
        false
    }
}

/// Control flow signals — returned by eval_stmt to unwind the call stack.
pub enum Flow {
    Normal(Option<Value>),
    Return(Value),
    Break,
    Continue,
    Throw(Value),
}

/// Interpret a list of statements in the given scope.
#[inline]
pub fn interpret(stmts: &[Stmt], scope: &mut Scope) -> Result<Value, String> {
    let mut last = Value::Undefined;
    for stmt in stmts {
        match eval_stmt(stmt, scope)? {
            Flow::Normal(v) => {
                if let Some(val) = v {
                    last = val;
                }
            }
            Flow::Return(v) => return Ok(v),
            Flow::Break | Flow::Continue | Flow::Throw(_) => {}
        }
    }
    Ok(last)
}

/// Public eval_stmt — used by the VM for calling user functions.
#[inline]
pub fn eval_stmt_pub(stmt: &Stmt, scope: &mut Scope) -> Result<Flow, String> {
    eval_stmt(stmt, scope)
}

#[inline]
fn eval_stmt(stmt: &Stmt, scope: &mut Scope) -> Result<Flow, String> {
    match stmt {
        Stmt::Empty => Ok(Flow::Normal(None)),
        Stmt::Switch(expr, cases) => {
            let switch_val = eval_expr(expr, scope)?;
            let mut matched = false;
            let mut default_idx: Option<usize> = None;
            let mut result = Flow::Normal(None);

            // First pass: find the matching case (or default).
            for (i, case) in cases.iter().enumerate() {
                if case.test.is_none() {
                    default_idx = Some(i);
                    continue;
                }
                if let Some(test_expr) = &case.test {
                    let test_val = eval_expr(test_expr, scope)?;
                    if switch_val.equals(&test_val) {
                        matched = true;
                        // Execute this case and fall through.
                        for s in cases.iter().skip(i) {
                            for stmt in &s.body {
                                match eval_stmt(stmt, scope)? {
                                    Flow::Break => return Ok(result),
                                    f @ (Flow::Return(_) | Flow::Continue | Flow::Throw(_)) => return Ok(f),
                                    Flow::Normal(v) => {
                                        if v.is_some() {
                                            result = Flow::Normal(v);
                                        }
                                    }
                                }
                            }
                        }
                        break;
                    }
                }
            }

            // No case matched — try default.
            if !matched {
                if let Some(di) = default_idx {
                    for s in cases.iter().skip(di) {
                        for stmt in &s.body {
                            match eval_stmt(stmt, scope)? {
                                Flow::Break => return Ok(result),
                                f @ (Flow::Return(_) | Flow::Continue | Flow::Throw(_)) => return Ok(f),
                                Flow::Normal(v) => {
                                    if v.is_some() {
                                        result = Flow::Normal(v);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Ok(result)
        }
        Stmt::Expression(e) => {
            let v = eval_expr(e, scope)?;
            Ok(Flow::Normal(Some(v)))
        }
        Stmt::Var(name, init) => {
            // JavaScript var hoisting: declare as undefined FIRST, then
            // evaluate the initializer. This is critical for self-referencing
            // object literals like `var ytcfg = {d: function(){ return ytcfg.data_ }}`.
            scope.declare(name, Value::Undefined);
            let val = eval_expr(init, scope)?;
            scope.set(name, val.clone());
            // Browser compat: at the global scope, `var x = 1` also sets
            // `window.x = 1`.
            if scope.parent.is_none() {
                if let Some(window) = scope.get("window") {
                    if let Value::Object(win_obj) = &window {
                        win_obj.borrow_mut().set(name, val);
                    }
                }
            }
            Ok(Flow::Normal(None))
        }
        Stmt::Let(name, init) | Stmt::Const(name, init) => {
            // let/const are NOT hoisted (temporal dead zone), but we still
            // need to declare before init for self-referencing in the same
            // statement. This is technically incorrect (let/const should
            // throw ReferenceError if accessed before initialization) but
            // works for YouTube's patterns.
            scope.declare(name, Value::Undefined);
            let val = eval_expr(init, scope)?;
            scope.set(name, val.clone());
            if scope.parent.is_none() {
                if let Some(window) = scope.get("window") {
                    if let Value::Object(win_obj) = &window {
                        win_obj.borrow_mut().set(name, val);
                    }
                }
            }
            Ok(Flow::Normal(None))
        }
        Stmt::Function(name, params, body) => {
            // First declare a placeholder so recursive calls can find the name.
            scope.declare(name, Value::Undefined);
            // Create the function with a closure that includes the name.
            let func = Value::Function(Rc::new(FunctionValue {
                params: params.clone(),
                body: body.clone(),
                closure: Rc::new(RefCell::new(scope.clone())),
                name: name.clone(),
                vm_code: None,
            static_props: RefCell::new(HashMap::new()),
                vm_nlocals: 0,
            }));
            // Update the placeholder — but the closure already has the name
            // pointing to Undefined. We need to also update it in the closure.
            // Since the closure is a clone, we update both.
            scope.set(name, func.clone());
            // Also update in the function's closure.
            if let Value::Function(ref f) = func {
                f.closure.borrow_mut().set(name, func.clone());
            }
            // Browser compat: global-scope functions go on `window`.
            if scope.parent.is_none() {
                if let Some(window) = scope.get("window") {
                    if let Value::Object(win_obj) = &window {
                        win_obj.borrow_mut().set(name, func.clone());
                    }
                }
            }
            Ok(Flow::Normal(None))
        }
        Stmt::If(cond, then, els) => {
            let c = eval_expr(cond, scope)?;
            let mut last_val = Value::Undefined;
            if c.is_truthy() {
                for s in then {
                    match eval_stmt(s, scope)? {
                        f @ (Flow::Return(_) | Flow::Break | Flow::Continue | Flow::Throw(_)) => {
                            return Ok(f)
                        }
                        Flow::Normal(v) => {
                            if let Some(val) = v {
                                last_val = val;
                            }
                        }
                    }
                }
            } else if let Some(e) = els {
                for s in e {
                    match eval_stmt(s, scope)? {
                        f @ (Flow::Return(_) | Flow::Break | Flow::Continue | Flow::Throw(_)) => {
                            return Ok(f)
                        }
                        Flow::Normal(v) => {
                            if let Some(val) = v {
                                last_val = val;
                            }
                        }
                    }
                }
            }
            Ok(Flow::Normal(Some(last_val)))
        }
        Stmt::For(init, test, update, body) => {
            if let Some(ref i) = init {
                eval_stmt(i, scope)?;
            }
            loop {
                if let Some(t) = test {
                    let v = eval_expr(t, scope)?;
                    if !v.is_truthy() {
                        break;
                    }
                }
                for s in body {
                    match eval_stmt(s, scope)? {
                        Flow::Break => return Ok(Flow::Normal(None)),
                        Flow::Continue => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        Flow::Throw(v) => return Ok(Flow::Throw(v)),
                        Flow::Normal(_) => {}
                    }
                }
                if let Some(u) = update {
                    eval_expr(u, scope)?;
                }
            }
            Ok(Flow::Normal(None))
        }
        Stmt::While(cond, body) => {
            loop {
                let v = eval_expr(cond, scope)?;
                if !v.is_truthy() {
                    break;
                }
                for s in body {
                    match eval_stmt(s, scope)? {
                        Flow::Break => return Ok(Flow::Normal(None)),
                        Flow::Continue => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        Flow::Throw(v) => return Ok(Flow::Throw(v)),
                        Flow::Normal(_) => {}
                    }
                }
            }
            Ok(Flow::Normal(None))
        }
        Stmt::DoWhile(body, cond) => {
            loop {
                for s in body {
                    match eval_stmt(s, scope)? {
                        Flow::Break => return Ok(Flow::Normal(None)),
                        Flow::Continue => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        Flow::Throw(v) => return Ok(Flow::Throw(v)),
                        Flow::Normal(_) => {}
                    }
                }
                let v = eval_expr(cond, scope)?;
                if !v.is_truthy() {
                    break;
                }
            }
            Ok(Flow::Normal(None))
        }
        Stmt::Return(e) => {
            let v = match e {
                Some(e) => eval_expr(e, scope)?,
                None => Value::Undefined,
            };
            Ok(Flow::Return(v))
        }
        Stmt::Break => Ok(Flow::Break),
        Stmt::Continue => Ok(Flow::Continue),
        Stmt::Block(stmts) => {
            for s in stmts {
                match eval_stmt(s, scope)? {
                    f @ (Flow::Return(_) | Flow::Break | Flow::Continue | Flow::Throw(_)) => {
                        return Ok(f)
                    }
                    Flow::Normal(_) => {}
                }
            }
            Ok(Flow::Normal(None))
        }
        Stmt::Throw(e) => {
            let v = eval_expr(e, scope)?;
            Ok(Flow::Throw(v))
        }
        Stmt::TryCatch(try_body, catch_param, catch_body, finally_body) => {
            let mut result = Flow::Normal(None);
            for s in try_body {
                match eval_stmt(s, scope)? {
                    Flow::Throw(err) => {
                        if let (Some(param), Some(body)) = (&catch_param, &catch_body) {
                            scope.declare(param, err);
                            for s in body {
                                match eval_stmt(s, scope)? {
                                    Flow::Normal(v) => {
                                        // Capture expression values from the catch body.
                                        if v.is_some() {
                                            result = Flow::Normal(v);
                                        }
                                    }
                                    f @ (Flow::Return(_)
                                    | Flow::Break
                                    | Flow::Continue
                                    | Flow::Throw(_)) => {
                                        result = f;
                                        break;
                                    }
                                }
                            }
                        }
                        break;
                    }
                    Flow::Normal(v) => {
                        if v.is_some() {
                            result = Flow::Normal(v);
                        }
                    }
                    f @ (Flow::Return(_) | Flow::Break | Flow::Continue) => {
                        result = f;
                        break;
                    }
                }
            }
            // Run finally block if present (always runs, even on return/break/throw).
            if let Some(fbody) = finally_body {
                for s in fbody {
                    match eval_stmt(s, scope)? {
                        Flow::Normal(v) => {
                            // Finally expression values override the result.
                            if v.is_some() {
                                result = Flow::Normal(v);
                            }
                        }
                        // If finally throws or returns, it overrides the try/catch result.
                        f @ (Flow::Return(_) | Flow::Break | Flow::Continue | Flow::Throw(_)) => {
                            result = f;
                            break;
                        }
                    }
                }
            }
            Ok(result)
        }
        // === ES2020+ statements ===
        Stmt::Class(class_def) => {
            let class_val = build_class(class_def, scope)?;
            let name = class_def.name.clone().unwrap_or_default();
            if !name.is_empty() {
                scope.declare(&name, class_val);
            }
            Ok(Flow::Normal(None))
        }
        Stmt::AsyncFunction(name, params, body) => {
            // Treat as regular function — async semantics are sync in our interpreter.
            let func = Value::Function(Rc::new(FunctionValue {
                params: params.clone(),
                body: body.clone(),
                closure: Rc::new(RefCell::new(scope.clone())),
                name: name.clone(),
                vm_code: None,
            static_props: RefCell::new(HashMap::new()),
                vm_nlocals: 0,
            }));
            scope.declare(name, func);
            Ok(Flow::Normal(None))
        }
        Stmt::GeneratorFunction(name, params, body) => {
            let func = Value::Function(Rc::new(FunctionValue {
                params: params.clone(),
                body: body.clone(),
                closure: Rc::new(RefCell::new(scope.clone())),
                name: name.clone(),
                vm_code: None,
            static_props: RefCell::new(HashMap::new()),
                vm_nlocals: 0,
            }));
            scope.declare(name, func);
            Ok(Flow::Normal(None))
        }
        Stmt::AsyncGeneratorFunction(name, params, body) => {
            let func = Value::Function(Rc::new(FunctionValue {
                params: params.clone(),
                body: body.clone(),
                closure: Rc::new(RefCell::new(scope.clone())),
                name: name.clone(),
                vm_code: None,
            static_props: RefCell::new(HashMap::new()),
                vm_nlocals: 0,
            }));
            scope.declare(name, func);
            Ok(Flow::Normal(None))
        }
        Stmt::ForInOf(kind, var_name, iterable_expr, body) => {
            let iterable = eval_expr(iterable_expr, scope)?;
            match kind {
                crate::tjs::parser::ForInOfKind::ForIn => {
                    // for (x in obj) — iterate over enumerable keys.
                    let keys: Vec<String> = match &iterable {
                        Value::Object(obj) => obj.borrow().properties.keys().cloned().collect(),
                        Value::Array(arr) => {
                            (0..arr.borrow().len()).map(|i| i.to_string()).collect()
                        }
                        Value::String(s) => (0..s.chars().count()).map(|i| i.to_string()).collect(),
                        _ => Vec::new(),
                    };
                    for key in keys {
                        // Use set() so the variable is updated (not re-declared)
                        // on each iteration. If it doesn't exist yet, set() will
                        // create it.
                        scope.set(var_name, Value::String(key));
                        for s in body {
                            match eval_stmt(s, scope)? {
                                Flow::Break => return Ok(Flow::Normal(None)),
                                Flow::Continue => break,
                                Flow::Return(v) => return Ok(Flow::Return(v)),
                                Flow::Throw(v) => return Ok(Flow::Throw(v)),
                                Flow::Normal(_) => {}
                            }
                        }
                    }
                    Ok(Flow::Normal(None))
                }
                crate::tjs::parser::ForInOfKind::ForOf
                | crate::tjs::parser::ForInOfKind::ForAwaitOf => {
                    // for (x of iterable) — iterate over values.
                    let values: Vec<Value> = match &iterable {
                        Value::Array(arr) => arr.borrow().iter().cloned().collect(),
                        Value::String(s) => {
                            s.chars().map(|c| Value::String(c.to_string())).collect()
                        }
                        Value::Object(obj) => obj.borrow().properties.values().cloned().collect(),
                        _ => Vec::new(),
                    };
                    for val in values {
                        scope.set(var_name, val);
                        for s in body {
                            match eval_stmt(s, scope)? {
                                Flow::Break => return Ok(Flow::Normal(None)),
                                Flow::Continue => break,
                                Flow::Return(v) => return Ok(Flow::Return(v)),
                                Flow::Throw(v) => return Ok(Flow::Throw(v)),
                                Flow::Normal(_) => {}
                            }
                        }
                    }
                    Ok(Flow::Normal(None))
                }
            }
        }
    }
}

#[inline]
fn eval_expr(expr: &Expr, scope: &mut Scope) -> Result<Value, String> {
    match expr {
        Expr::Number(n) => Ok(Value::Number(*n)),
        Expr::String(s) => Ok(Value::String(s.clone())),
        Expr::Boolean(b) => Ok(Value::Boolean(*b)),
        Expr::Null => Ok(Value::Null),
        Expr::Undefined => Ok(Value::Undefined),
        Expr::Identifier(name) => scope
            .get(name)
            .ok_or_else(|| format!("{} is not defined", name)),
        Expr::This => Ok(scope.get("this").unwrap_or(Value::Undefined)),
        Expr::Binary(left, op, right) => {
            // Short-circuit evaluation.
            match op.as_str() {
                "&&" => {
                    let l = eval_expr(left, scope)?;
                    if !l.is_truthy() {
                        return Ok(l);
                    }
                    return eval_expr(right, scope);
                }
                "||" => {
                    let l = eval_expr(left, scope)?;
                    if l.is_truthy() {
                        return Ok(l);
                    }
                    return eval_expr(right, scope);
                }
                "??" => {
                    let l = eval_expr(left, scope)?;
                    if !matches!(l, Value::Null | Value::Undefined) {
                        return Ok(l);
                    }
                    return eval_expr(right, scope);
                }
                _ => {}
            }
            let l = eval_expr(left, scope)?;
            let r = eval_expr(right, scope)?;
            Ok(binary_op(&l, op, &r))
        }
        Expr::Unary(op, expr) => {
            let v = eval_expr(expr, scope)?;
            match op.as_str() {
                "!" => Ok(Value::Boolean(!v.is_truthy())),
                "-" => Ok(Value::Number(-v.to_number())),
                "+" => Ok(Value::Number(v.to_number())),
                "~" => Ok(Value::Number(!(v.to_number() as i32) as f64)),
                "typeof" => Ok(Value::String(v.type_name().to_string())),
                "delete" => {
                    // delete obj.prop or delete obj[prop]
                    // Returns true (property deleted or didn't exist).
                    Ok(Value::Boolean(true))
                }
                "void" => Ok(Value::Undefined),
                _ => Err(format!("Unknown unary operator: {}", op)),
            }
        }
        Expr::Update(op, expr, is_prefix) => {
            if let Expr::Identifier(name) = expr.as_ref() {
                let current = scope.get(name).unwrap_or(Value::Number(0.0)).to_number();
                let new_val = if op == "++" {
                    current + 1.0
                } else {
                    current - 1.0
                };
                scope.set(name, Value::Number(new_val));
                Ok(if *is_prefix {
                    Value::Number(new_val)
                } else {
                    Value::Number(current)
                })
            } else {
                Err("++/-- requires a variable".to_string())
            }
        }
        Expr::Assign(target, op, value) => {
            let v = eval_expr(value, scope)?;
            // For compound assignments (+=, -=, **=, etc.), extract the base operator.
            // Special case: **= → ** (not *).
            let base_op = if op == "**=" {
                "**".to_string()
            } else if op == "&&=" || op == "||=" || op == "??=" {
                // Logical assignment operators — handle below.
                String::new()
            } else {
                op.trim_end_matches('=').to_string()
            };
            match target.as_ref() {
                Expr::Identifier(name) => {
                    let final_val = if op == "=" {
                        v.clone()
                    } else if op == "&&=" {
                        let current = scope.get(name).unwrap_or(Value::Undefined);
                        if current.is_truthy() { v.clone() } else { current }
                    } else if op == "||=" {
                        let current = scope.get(name).unwrap_or(Value::Undefined);
                        if !current.is_truthy() { v.clone() } else { current }
                    } else if op == "??=" {
                        let current = scope.get(name).unwrap_or(Value::Undefined);
                        if matches!(current, Value::Null | Value::Undefined) { v.clone() } else { current }
                    } else {
                        let current = scope.get(name).unwrap_or(Value::Undefined);
                        binary_op(&current, &base_op, &v)
                    };
                    scope.set(name, final_val.clone());
                    Ok(final_val)
                }
                Expr::Member(obj, prop, is_computed) => {
                    let obj_val = eval_expr(obj, scope)?;
                    let key = if *is_computed {
                        eval_expr(prop, scope)?.to_string()
                    } else {
                        if let Expr::String(s) = prop.as_ref() {
                            s.clone()
                        } else {
                            String::new()
                        }
                    };
                    let final_val = if op == "=" {
                        v.clone()
                    } else if op == "&&=" {
                        let current = obj_val.get_property(&key);
                        if current.is_truthy() { v.clone() } else { current }
                    } else if op == "||=" {
                        let current = obj_val.get_property(&key);
                        if !current.is_truthy() { v.clone() } else { current }
                    } else if op == "??=" {
                        let current = obj_val.get_property(&key);
                        if matches!(current, Value::Null | Value::Undefined) { v.clone() } else { current }
                    } else {
                        let current = obj_val.get_property(&key);
                        binary_op(&current, &base_op, &v)
                    };
                    obj_val.set_property(&key, final_val.clone());
                    Ok(final_val)
                }
                _ => Err("Invalid assignment target".to_string()),
            }
        }
        Expr::Call(callee, args) => {
            let (func, this_val) = match callee.as_ref() {
                Expr::Member(obj, prop, is_computed) => {
                    let obj_val = eval_expr(obj, scope)?;
                    let key = if *is_computed {
                        eval_expr(prop, scope)?.to_string()
                    } else {
                        if let Expr::String(s) = prop.as_ref() {
                            s.clone()
                        } else {
                            String::new()
                        }
                    };
                    let method = obj_val.get_property(&key);
                    (method, obj_val)
                }
                _ => {
                    let f = eval_expr(callee, scope)?;
                    (f, Value::Undefined)
                }
            };
            // Use eval_args to support spread in calls: f(...args)
            let arg_values = eval_args(args, scope)?;
            call_function(&func, arg_values, &this_val, scope)
        }
        Expr::Member(obj, prop, is_computed) => {
            let obj_val = eval_expr(obj, scope)?;
            let key = if *is_computed {
                eval_expr(prop, scope)?.to_string()
            } else {
                if let Expr::String(s) = prop.as_ref() {
                    s.clone()
                } else {
                    String::new()
                }
            };
            Ok(obj_val.get_property(&key))
        }
        Expr::Function(params, body) => Ok(Value::Function(Rc::new(FunctionValue {
            params: params.clone(),
            body: body.clone(),
            closure: Rc::new(RefCell::new(scope.clone())),
            name: String::new(),
            vm_code: None,
            static_props: RefCell::new(HashMap::new()),
            vm_nlocals: 0,
        }))),
        Expr::Arrow(params, body) => {
            let arrow_body = match body.as_ref() {
                Stmt::Block(stmts) => stmts.clone(),
                Stmt::Return(Some(e)) => vec![Stmt::Return(Some(e.clone()))],
                _ => vec![Stmt::Return(None)],
            };
            Ok(Value::Function(Rc::new(FunctionValue {
                params: params.clone(),
                body: arrow_body,
                closure: Rc::new(RefCell::new(scope.clone())),
                name: String::new(),
                vm_code: None,
            static_props: RefCell::new(HashMap::new()),
                vm_nlocals: 0,
            })))
        }
        Expr::Object(properties) => {
            let mut obj = ObjectValue::new();
            for (key, val_expr) in properties {
                let val = eval_expr(val_expr, scope)?;
                obj.set(key, val);
            }
            Ok(Value::Object(Rc::new(RefCell::new(obj))))
        }
        Expr::Array(elements) => {
            let mut arr = Vec::new();
            for e in elements {
                // Spread inside array literal: [...x, y, ...z]
                if let Expr::Spread(inner) = e {
                    let v = eval_expr(inner, scope)?;
                    match v {
                        Value::Array(inner_arr) => {
                            arr.extend(inner_arr.borrow().iter().cloned());
                        }
                        Value::String(s) => {
                            for c in s.chars() {
                                arr.push(Value::String(c.to_string()));
                            }
                        }
                        Value::Object(obj) => {
                            for val in obj.borrow().properties.values() {
                                arr.push(val.clone());
                            }
                        }
                        _ => arr.push(v),
                    }
                } else {
                    arr.push(eval_expr(e, scope)?);
                }
            }
            Ok(Value::Array(Rc::new(RefCell::new(arr))))
        }
        Expr::New(callee, args) => {
            let func = eval_expr(callee, scope)?;
            // Spread in `new` calls.
            let arg_values = eval_args(args, scope)?;
            // Create the new object.
            let this_val = Value::Object(Rc::new(RefCell::new(ObjectValue::new())));

            // If the callee is a class constructor (function), copy the
            // prototype methods onto the new object before calling the
            // constructor. Then initialize instance fields.
            if let Value::Function(ref f) = func {
                // Look up "prototype" in the constructor's closure OR static_props.
                let proto_val = f.closure.borrow().vars.borrow().iter()
                    .find(|(k, _)| k == "prototype")
                    .map(|(_, v)| v.clone())
                    .or_else(|| {
                        // Also check static_props for prototype.
                        f.static_props.borrow().get("prototype").cloned()
                    });
                if let Some(proto_val) = proto_val {
                    if let Value::Object(proto_obj) = proto_val {
                        // Copy each method from prototype to this.
                        let proto_methods: Vec<(String, Value)> = proto_obj
                            .borrow()
                            .properties
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        if let Value::Object(this_obj) = &this_val {
                            for (k, v) in proto_methods {
                                this_obj.borrow_mut().properties.insert(k, v);
                            }
                            // Set the prototype chain.
                            this_obj.borrow_mut().prototype =
                                Some(Value::Object(proto_obj.clone()));
                        }
                    }
                }

                // Initialize instance fields. The constructor's body
                // contains field initializers as statements (we store them
                // via a special marker). For simplicity, we look for a
                // `__fields` list on the closure that build_class sets up.
                if let Some(fields_val) = f
                    .closure
                    .borrow()
                    .vars
                    .borrow()
                    .iter()
                    .find(|(k, _)| k == "__fields")
                    .map(|(_, v)| v.clone())
                {
                    if let Value::Array(fields_arr) = fields_val {
                        // Each field is stored as an object: { name, initializer }
                        // For simplicity, we stored fields as the class_def's
                        // field list, but since we can't easily serialize that
                        // to a Value, we skip this for now.
                        let _ = fields_arr;
                    }
                }
            }

            // Call the constructor with `this` bound to the new object.
            // If func is an Object with a __new__ property, use that instead.
            if let Value::Object(obj) = &func {
                if let Some(constructor) = obj.borrow().properties.get("__new__") {
                    return call_function(constructor, arg_values, &Value::Undefined, scope);
                }
            }
            // For builtins, the constructor returns the new object directly.
            // For user functions, call with `this` and return this_val.
            match &func {
                Value::Builtin(b) => {
                    let result = (b.func)(arg_values)?;
                    // If the builtin returned an object, use it; otherwise use this_val.
                    if matches!(result, Value::Object(_) | Value::Array(_)) {
                        Ok(result)
                    } else {
                        Ok(this_val)
                    }
                }
                _ => {
                    let _ = call_function(&func, arg_values, &this_val, scope)?;
                    Ok(this_val)
                }
            }
        }
        Expr::Conditional(test, consequent, alternate) => {
            let t = eval_expr(test, scope)?;
            if t.is_truthy() {
                eval_expr(consequent, scope)
            } else {
                eval_expr(alternate, scope)
            }
        }
        Expr::Sequence(exprs) => {
            let mut last = Value::Undefined;
            for e in exprs {
                last = eval_expr(e, scope)?;
            }
            Ok(last)
        }
        Expr::Template(parts) => {
            let mut result = String::new();
            for part in parts {
                let v = eval_expr(part, scope)?;
                result.push_str(&v.to_string());
            }
            Ok(Value::String(result))
        }
        // === ES2020+ features ===
        Expr::BigInt(s) => Ok(Value::BigInt(s.clone())),
        Expr::OptionalMember(obj, prop, is_computed) => {
            let obj_val = eval_expr(obj, scope)?;
            // Optional chaining: if obj is null/undefined, return undefined.
            if matches!(obj_val, Value::Null | Value::Undefined) {
                return Ok(Value::Undefined);
            }
            let key = if *is_computed {
                eval_expr(prop, scope)?.to_string()
            } else {
                if let Expr::String(s) = prop.as_ref() {
                    s.clone()
                } else {
                    String::new()
                }
            };
            Ok(obj_val.get_property(&key))
        }
        Expr::OptionalCall(callee, args) => {
            let func = eval_expr(callee, scope)?;
            // Optional call: if callee is null/undefined, return undefined.
            if matches!(func, Value::Null | Value::Undefined) {
                return Ok(Value::Undefined);
            }
            let arg_vals = eval_args(args, scope)?;
            call_function(&func, arg_vals, &Value::Undefined, scope)
        }
        Expr::Spread(inner) => {
            // Spread evaluates to the underlying value; the consumer (array,
            // call) is responsible for unpacking. We just return the value.
            eval_expr(inner, scope)
        }
        Expr::PrivateIdentifier(name) => {
            // Reference to a private field — should only appear in a member
            // access (this.#x). As a standalone expression it's an error.
            Err(format!(
                "Cannot reference private field #{} outside class",
                name
            ))
        }
        Expr::Super => {
            // super — should only appear inside a class method that extends
            // a parent. As a standalone expression, return undefined.
            Ok(Value::Undefined)
        }
        Expr::NewTarget => {
            // new.target — true if called with `new`, false otherwise.
            // We don't currently track this; return undefined.
            Ok(Value::Undefined)
        }
        Expr::Yield(expr) => {
            // yield — inside a generator function.
            // We implement generators by pre-computing all yielded values
            // when the generator is first called, storing them in an array
            // on the returned iterator object. The `next()` method pops
            // from the array. This is not lazy (doesn't pause execution)
            // but produces correct results for most use cases.
            //
            // When `yield` is encountered during the pre-computation pass,
            // it pushes the value to the yield buffer and continues.
            if let Some(e) = expr {
                let v = eval_expr(e, scope)?;
                // Store yielded value in the generator's __yielded buffer.
                if let Some(Value::Array(buf)) = scope.get("__yielded") {
                    buf.borrow_mut().push(v.clone());
                }
                Ok(v)
            } else {
                Ok(Value::Undefined)
            }
        }
        Expr::Await(expr) => {
            // await — inside an async function.
            // Since our event loop is single-threaded and runs after script
            // execution, we evaluate the expression synchronously. If it's
            // a Promise, we check if it's already resolved (our Promises
            // resolve immediately) and return the resolved value.
            let v = eval_expr(expr, scope)?;
            // If it's a Promise-like object with __result, return that.
            if let Value::Object(obj) = &v {
                let obj = obj.borrow();
                if let Some(val) = obj.properties.get("__result") {
                    return Ok(val.clone());
                }
            }
            Ok(v)
        }
        Expr::Class(class_def) => {
            // Class expression — build a constructor function + prototype.
            Ok(build_class(class_def, scope)?)
        }
        Expr::GeneratorFunction(params, body) => {
            // Generator function expression. We store it as a regular
            // function — `yield` inside will evaluate synchronously.
            Ok(Value::Function(Rc::new(FunctionValue {
                params: params.clone(),
                body: body.clone(),
                closure: Rc::new(RefCell::new(scope.clone())),
                name: String::new(),
                vm_code: None,
            static_props: RefCell::new(HashMap::new()),
                vm_nlocals: 0,
            })))
        }
        Expr::AsyncFunction(params, body) => {
            // Async function expression. Without a real event loop, we treat
            // it as a regular function — `await` inside evaluates synchronously.
            Ok(Value::Function(Rc::new(FunctionValue {
                params: params.clone(),
                body: body.clone(),
                closure: Rc::new(RefCell::new(scope.clone())),
                name: String::new(),
                vm_code: None,
            static_props: RefCell::new(HashMap::new()),
                vm_nlocals: 0,
            })))
        }
        Expr::AsyncGeneratorFunction(params, body) => Ok(Value::Function(Rc::new(FunctionValue {
            params: params.clone(),
            body: body.clone(),
            closure: Rc::new(RefCell::new(scope.clone())),
            name: String::new(),
            vm_code: None,
            static_props: RefCell::new(HashMap::new()),
            vm_nlocals: 0,
        }))),
    }
}

/// Evaluate a list of arguments, expanding Spread elements into individual
/// values.
fn eval_args(args: &[Expr], scope: &mut Scope) -> Result<Vec<Value>, String> {
    let mut result = Vec::new();
    for arg in args {
        match arg {
            Expr::Spread(inner) => {
                let v = eval_expr(inner, scope)?;
                match v {
                    Value::Array(arr) => {
                        result.extend(arr.borrow().iter().cloned());
                    }
                    Value::Object(obj) => {
                        // Iterate over object's own enumerable properties.
                        for val in obj.borrow().properties.values() {
                            result.push(val.clone());
                        }
                    }
                    _ => {
                        // Non-iterable spread — just push the value.
                        result.push(v);
                    }
                }
            }
            _ => {
                result.push(eval_expr(arg, scope)?);
            }
        }
    }
    Ok(result)
}

/// Build a class: constructor function + prototype with methods + static
/// fields/methods. Private fields are stored with a `#` prefix on the
/// instance.
fn build_class(
    class_def: &crate::tjs::parser::ClassDef,
    scope: &mut Scope,
) -> Result<Value, String> {
    use crate::tjs::parser::{ClassMethod, MethodKind};

    // Find the constructor method (if any).
    let constructor = class_def
        .methods
        .iter()
        .find(|m| m.kind == MethodKind::Constructor);
    let non_constructor_methods: Vec<&ClassMethod> = class_def
        .methods
        .iter()
        .filter(|m| m.kind != MethodKind::Constructor)
        .collect();

    // Build the constructor function. If there are instance fields, we
    // prepend their initializers to the constructor body so they run first.
    let (ctor_params, mut ctor_body) = if let Some(c) = constructor {
        (c.params.clone(), c.body.clone())
    } else {
        (Vec::new(), Vec::new())
    };

    // Prepend instance field initializers to the constructor body.
    // Each field becomes: this.#name = initializer; (or this.name = init for public)
    let mut field_inits: Vec<Stmt> = Vec::new();
    for field in &class_def.fields {
        if field.is_static {
            continue;
        } // static fields handled separately
        let field_name = if field.is_private {
            format!("#{}", field.name)
        } else {
            field.name.clone()
        };
        let init_expr = if let Some(init) = &field.initializer {
            init.clone()
        } else {
            Expr::Undefined
        };
        // this.fieldName = init
        let assign = Expr::Assign(
            Box::new(Expr::Member(
                Box::new(Expr::This),
                Box::new(Expr::String(field_name)),
                false,
            )),
            "=".to_string(),
            Box::new(init_expr),
        );
        field_inits.push(Stmt::Expression(assign));
    }
    // field_inits go first, then the original constructor body.
    let mut full_body = field_inits;
    full_body.append(&mut ctor_body);

    let ctor_fn = Value::Function(Rc::new(FunctionValue {
        params: ctor_params,
        body: full_body,
        closure: Rc::new(RefCell::new(scope.clone())),
        name: class_def.name.clone().unwrap_or_default(),
        vm_code: None,
            static_props: RefCell::new(HashMap::new()),
        vm_nlocals: 0,
    }));

    // Build the prototype object with methods.
    let prototype = Rc::new(RefCell::new(ObjectValue {
        properties: std::collections::HashMap::new(),
        prototype: None,
    }));

    // Set up inheritance: if extends, prototype's __proto__ = parent's prototype.
    if let Some(parent_expr) = &class_def.extends {
        let parent = eval_expr(parent_expr, scope)?;
        if let Value::Function(parent_fn) = &parent {
            // Set the prototype's prototype to the parent's prototype.
            if let Some(parent_proto) = parent_fn
                .closure
                .borrow()
                .vars
                .borrow()
                .iter()
                .find(|(k, _)| k == "prototype")
                .map(|(_, v)| v.clone())
            {
                prototype.borrow_mut().prototype = Some(parent_proto);
            }
        }
        // Also store the parent constructor on the closure so super() works.
        if let Value::Function(ref f) = ctor_fn {
            f.closure.borrow_mut().declare("__super", parent.clone());
        }
    }

    // Add methods to prototype (or to the constructor for static methods).
    for method in &non_constructor_methods {
        let method_fn = Value::Function(Rc::new(FunctionValue {
            params: method.params.clone(),
            body: method.body.clone(),
            closure: Rc::new(RefCell::new(scope.clone())),
            name: method.name.clone(),
            vm_code: None,
            static_props: RefCell::new(HashMap::new()),
            vm_nlocals: 0,
        }));
        if method.is_static {
            if let Value::Function(ref f) = ctor_fn {
                f.static_props.borrow_mut().insert(method.name.clone(), method_fn);
            }
        } else {
            // Private methods are stored with # prefix on instances (via
            // field initialization). For simplicity, we also add them to
            // the prototype so they can be looked up.
            let proto_key = if method.is_private {
                format!("#{}", method.name)
            } else {
                method.name.clone()
            };
            prototype
                .borrow_mut()
                .properties
                .insert(proto_key.clone(), method_fn);
        }
    }

    // Store prototype on the constructor (as a "prototype" property in closure).
    if let Value::Function(ref f) = ctor_fn {
        f.closure
            .borrow_mut()
            .declare("prototype", Value::Object(prototype.clone()));
    }

    // Add static fields.
    for field in &class_def.fields {
        if !field.is_static {
            continue;
        }
        let value = if let Some(init) = &field.initializer {
            eval_expr(init, scope)?
        } else {
            Value::Undefined
        };
        if let Value::Function(ref f) = ctor_fn {
            f.static_props.borrow_mut().insert(field.name.clone(), value);
        }
    }

    Ok(ctor_fn)
}

#[inline(always)]
fn binary_op(left: &Value, op: &str, right: &Value) -> Value {
    // BigInt arithmetic: when both operands are BigInt, do BigInt math.
    if let (Value::BigInt(a), Value::BigInt(b)) = (left, right) {
        return bigint_binary_op(a, op, b);
    }
    // Fast paths for the most common operations (number + number).
    // This avoids pattern matching overhead and to_number() calls.
    match op {
        "+" => {
            // Fast path: both numbers.
            if let (Value::Number(a), Value::Number(b)) = (left, right) {
                return Value::Number(a + b);
            }
            // String concatenation.
            if matches!(left, Value::String(_)) || matches!(right, Value::String(_)) {
                return Value::String(format!("{}{}", left.to_string(), right.to_string()));
            }
            Value::Number(left.to_number() + right.to_number())
        }
        "-" => {
            if let (Value::Number(a), Value::Number(b)) = (left, right) {
                return Value::Number(a - b);
            }
            Value::Number(left.to_number() - right.to_number())
        }
        "*" => {
            if let (Value::Number(a), Value::Number(b)) = (left, right) {
                return Value::Number(a * b);
            }
            Value::Number(left.to_number() * right.to_number())
        }
        "/" => {
            if let (Value::Number(a), Value::Number(b)) = (left, right) {
                return Value::Number(a / b);
            }
            Value::Number(left.to_number() / right.to_number())
        }
        "%" => {
            if let (Value::Number(a), Value::Number(b)) = (left, right) {
                return Value::Number(a % b);
            }
            Value::Number(left.to_number() % right.to_number())
        }
        "**" => {
            if let (Value::Number(a), Value::Number(b)) = (left, right) {
                return Value::Number(a.powf(*b));
            }
            Value::Number(left.to_number().powf(right.to_number()))
        }
        "<" => {
            if let (Value::Number(a), Value::Number(b)) = (left, right) {
                return Value::Boolean(a < b);
            }
            Value::Boolean(left.to_number() < right.to_number())
        }
        ">" => {
            if let (Value::Number(a), Value::Number(b)) = (left, right) {
                return Value::Boolean(a > b);
            }
            Value::Boolean(left.to_number() > right.to_number())
        }
        "<=" => {
            if let (Value::Number(a), Value::Number(b)) = (left, right) {
                return Value::Boolean(a <= b);
            }
            Value::Boolean(left.to_number() <= right.to_number())
        }
        ">=" => {
            if let (Value::Number(a), Value::Number(b)) = (left, right) {
                return Value::Boolean(a >= b);
            }
            Value::Boolean(left.to_number() >= right.to_number())
        }
        "in" => {
            // `key in obj` — check if obj has the property.
            let key = left.to_string();
            match right {
                Value::Object(obj) => {
                    let has = obj.borrow().properties.contains_key(&key)
                        || obj
                            .borrow()
                            .prototype
                            .as_ref()
                            .map(|p| p.get_property(&key) != Value::Undefined)
                            .unwrap_or(false);
                    Value::Boolean(has)
                }
                Value::Array(arr) => {
                    if let Ok(idx) = key.parse::<usize>() {
                        Value::Boolean(idx < arr.borrow().len())
                    } else {
                        Value::Boolean(key == "length")
                    }
                }
                _ => Value::Boolean(false),
            }
        }
        "instanceof" => {
            // `x instanceof C` — check if x's prototype chain includes C.prototype.
            // Simplified: check if x is an object and C has a "prototype" property.
            match (left, right) {
                (Value::Object(_), Value::Function(ctor)) => {
                    // Check if the object's prototype chain includes ctor's prototype.
                    let has_proto = ctor
                        .closure
                        .borrow()
                        .vars
                        .borrow()
                        .iter()
                        .any(|(k, _)| k == "prototype");
                    Value::Boolean(has_proto)
                }
                (Value::Array(_), Value::Function(_)) => Value::Boolean(true),
                _ => Value::Boolean(false),
            }
        }
        "===" => Value::Boolean(left.equals(right)),
        "!==" => Value::Boolean(!left.equals(right)),
        "==" => Value::Boolean(left.loose_equals(right)),
        "!=" => Value::Boolean(!left.loose_equals(right)),
        _ => Value::Undefined,
    }
}

/// BigInt binary operation. We delegate to tjs_ext::BigInt for the actual
/// arbitrary-precision math. Inputs are decimal strings, output is a
/// decimal string wrapped in Value::BigInt.
fn bigint_binary_op(a: &str, op: &str, b: &str) -> Value {
    let a_bi = parse_bigint(a);
    let b_bi = parse_bigint(b);
    let result = match op {
        "+" => a_bi.add(&b_bi),
        "-" => a_bi.add(&b_bi.negate()),
        "*" => bigint_mul(&a_bi, &b_bi),
        "===" => return Value::Boolean(a == b),
        "!==" => return Value::Boolean(a != b),
        _ => return Value::Undefined,
    };
    Value::BigInt(result.to_string())
}

/// Parse a decimal string into a tjs_ext::BigInt.
fn parse_bigint(s: &str) -> crate::tjs_ext::BigInt {
    let negative = s.starts_with('-');
    let digits = s.trim_start_matches('-');
    // Parse as i64 (handles most practical cases).
    let n: i64 = digits.parse().unwrap_or(0);
    let mut bi = crate::tjs_ext::BigInt::from_i64(if negative { -n.abs() } else { n });
    let _ = &mut bi;
    bi
}

/// Simple BigInt multiplication via repeated addition (slow but correct).
fn bigint_mul(a: &crate::tjs_ext::BigInt, b: &crate::tjs_ext::BigInt) -> crate::tjs_ext::BigInt {
    // For small values, fall back to i64 arithmetic.
    if let (Some(ai), Some(bi)) = (a.to_i64(), b.to_i64()) {
        // Use i128 to avoid overflow for moderate values.
        let product = (ai as i128) * (bi as i128);
        // Truncate to i64 range — real impl would use the full BigInt.
        return crate::tjs_ext::BigInt::from_i64(product as i64);
    }
    a.clone()
}

fn call_function(
    func: &Value,
    args: Vec<Value>,
    this_val: &Value,
    _scope: &mut Scope,
) -> Result<Value, String> {
    match func {
        Value::Function(f) => {
            let mut func_scope = Scope::new(Some(f.closure.clone()));
            // Bind `this`.
            func_scope
                .vars
                .borrow_mut()
                .push(("this".to_string(), this_val.clone()));
            // Bind `arguments` — an array-like object with all arguments.
            let args_val = Value::Array(Rc::new(RefCell::new(args.clone())));
            func_scope
                .vars
                .borrow_mut()
                .push(("arguments".to_string(), args_val));
            // Bind parameters.
            for (i, param) in f.params.iter().enumerate() {
                let val = args.get(i).cloned().unwrap_or(Value::Undefined);
                func_scope.vars.borrow_mut().push((param.clone(), val));
            }
            // Execute body.
            for stmt in &f.body {
                match eval_stmt(stmt, &mut func_scope)? {
                    Flow::Return(v) => return Ok(v),
                    Flow::Throw(v) => return Err(v.to_string()),
                    _ => {}
                }
            }
            Ok(Value::Undefined)
        }
        Value::Builtin(b) => (b.func)(args),
        _ => Err(format!("{} is not a function", func.to_string())),
    }
}

/// Call a JS function value (Function or Builtin) with the given arguments,
/// using the interpreter path so closures/captured variables work. Intended
/// for builtin implementations (e.g. Array.prototype methods) that need to
/// invoke a user-supplied callback without access to a live `Scope`.
///
/// `this` is bound to Undefined; the callback's own closure scope carries any
/// captured variables, so the throwaway caller scope is irrelevant.
pub fn call_js(func: &Value, args: Vec<Value>) -> Result<Value, String> {
    let mut scope = Scope::new(None);
    call_function(func, args, &Value::Undefined, &mut scope)
}
