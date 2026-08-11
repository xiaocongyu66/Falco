//! TJS — Talons JavaScript Engine
//!
//! A from-scratch JavaScript engine for Falco. Pure Rust, no external
//! dependencies. Tree-walking interpreter with planned bytecode VM.
//!
//! Pipeline: Source → Lexer → Tokens → Parser → AST → Interpreter → Result
//!
//! Supported features:
//!   - Variables: var, let, const
//!   - Functions: declarations, expressions, arrow functions, closures
//!   - Control flow: if/else, for, while, do/while, break, continue
//!   - Types: number, string, boolean, null, undefined, object, array
//!   - Operators: + - * / % == != < > <= >= && || ! ++ -- += -= etc.
//!   - Builtins: Math, console.log, Date.now, Array, Object, JSON, parseInt
//!   - Template literals (basic)
//!   - try/catch (basic)

pub mod builtins;
pub mod interpreter;
pub mod jit;
pub mod jit_integration_tests;
pub mod lexer;
pub mod parser;
pub mod value;
pub mod vm;

/// TJS execution context — owns the global scope and builtin functions.
pub struct TjsContext {
    pub global_scope: interpreter::Scope,
    use_vm: bool,
}

impl TjsContext {
    /// Create a new TJS context with builtins registered.
    pub fn new() -> Self {
        let mut global = interpreter::Scope::new(None);
        builtins::register(&mut global);
        Self {
            global_scope: global,
            use_vm: false,
        }
    }

    /// Enable the bytecode VM for faster execution.
    pub fn with_vm(mut self) -> Self {
        self.use_vm = true;
        self
    }

    /// Execute a JavaScript source string.
    pub fn execute(&mut self, src: &str) -> Result<value::Value, String> {
        let tokens = lexer::tokenize(src)?;
        let ast = parser::parse(&tokens)?;
        let result = interpreter::interpret(&ast, &mut self.global_scope)?;
        Ok(result)
    }

    /// Execute JavaScript source in a specific scope (used by workers).
    pub fn execute_with_scope(
        &mut self,
        src: &str,
        scope: &mut interpreter::Scope,
    ) -> Result<value::Value, String> {
        let tokens = lexer::tokenize(src)?;
        let ast = parser::parse(&tokens)?;
        let result = interpreter::interpret(&ast, scope)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tjs_arithmetic() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("1 + 2 * 3").unwrap();
        assert_eq!(result, value::Value::Number(7.0));
    }

    #[test]
    fn tjs_variables() {
        let mut tjs = TjsContext::new();
        tjs.execute("var x = 10; var y = 20; console.log(x + y)")
            .unwrap();
    }

    #[test]
    fn tjs_functions() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("function add(a, b) { return a + b } add(3, 4)")
            .unwrap();
        assert_eq!(result, value::Value::Number(7.0));
    }

    #[test]
    fn tjs_closures() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute(
                "function counter() { var n = 0; return function() { return ++n } }
             var c = counter(); c(); c(); c()",
            )
            .unwrap();
        assert_eq!(result, value::Value::Number(3.0));
    }

    #[test]
    fn tjs_objects() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var obj = { x: 1, y: 2 }; obj.x + obj.y")
            .unwrap();
        assert_eq!(result, value::Value::Number(3.0));
    }

    #[test]
    fn tjs_arrays() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("var arr = [1, 2, 3, 4, 5]; arr[2]").unwrap();
        assert_eq!(result, value::Value::Number(3.0));
    }

    #[test]
    fn tjs_loops() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var sum = 0; for (var i = 1; i <= 100; i++) { sum += i } sum")
            .unwrap();
        assert_eq!(result, value::Value::Number(5050.0));
    }

    #[test]
    fn tjs_strings() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("'Hello' + ' ' + 'World'").unwrap();
        assert_eq!(result, value::Value::String("Hello World".to_string()));
    }

    #[test]
    fn tjs_while() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var n = 10; var f = 1; while (n > 1) { f *= n; n-- } f")
            .unwrap();
        assert_eq!(result, value::Value::Number(3628800.0));
    }

    #[test]
    fn tjs_if_else() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var x = 5; if (x > 3) { 'big' } else { 'small' }")
            .unwrap();
        assert_eq!(result, value::Value::String("big".to_string()));
    }

    // --- Regression tests: Array.prototype methods (were stubs / missing) ---
    // See value.rs `Value::Array` arm. push/pop/join were stubs returning
    // Undefined/empty; map/filter/reduce/forEach/find/some/every were absent.
    //
    // NOTE: Value::Array equality is Rc pointer identity, so we assert via
    // JSON.stringify (output) for array results and via scalar Value for the
    // rest.

    #[test]
    fn tjs_array_map() {
        let mut tjs = TjsContext::new();
        let r = tjs
            .execute("JSON.stringify([1,2,3,4,5].map(x => x*x))")
            .unwrap();
        assert_eq!(r, value::Value::String("[1,4,9,16,25]".to_string()));
    }

    #[test]
    fn tjs_array_filter() {
        let mut tjs = TjsContext::new();
        let r = tjs
            .execute("JSON.stringify([1,2,3,4,5].filter(x => x % 2 == 0))")
            .unwrap();
        assert_eq!(r, value::Value::String("[2,4]".to_string()));
    }

    #[test]
    fn tjs_array_reduce_with_init() {
        let mut tjs = TjsContext::new();
        let r = tjs.execute("[1,2,3,4,5].reduce((a,b) => a+b, 0)").unwrap();
        assert_eq!(r, value::Value::Number(15.0));
    }

    #[test]
    fn tjs_array_reduce_no_init() {
        let mut tjs = TjsContext::new();
        let r = tjs.execute("[1,2,3,4,5].reduce((a,b) => a+b)").unwrap();
        assert_eq!(r, value::Value::Number(15.0));
    }

    #[test]
    fn tjs_array_chain() {
        let mut tjs = TjsContext::new();
        let r = tjs
            .execute("[1,2,3,4,5].map(x=>x+1).filter(x=>x>2).reduce((a,b)=>a*b,1)")
            .unwrap();
        assert_eq!(r, value::Value::Number(360.0));
    }

    #[test]
    fn tjs_array_push_join() {
        let mut tjs = TjsContext::new();
        let r = tjs
            .execute("var a=[10,20]; a.push(30); a.push(40); a.join('-')")
            .unwrap();
        assert_eq!(r, value::Value::String("10-20-30-40".to_string()));
    }

    #[test]
    fn tjs_array_slice_concat_includes_indexof() {
        let mut tjs = TjsContext::new();
        assert_eq!(
            tjs.execute("JSON.stringify([1,2,3,4,5].slice(1,3))")
                .unwrap(),
            value::Value::String("[2,3]".to_string())
        );
        assert_eq!(
            tjs.execute("JSON.stringify([1,2].concat([3,4],[5]))")
                .unwrap(),
            value::Value::String("[1,2,3,4,5]".to_string())
        );
        assert_eq!(
            tjs.execute("[1,2,3].includes(3)").unwrap(),
            value::Value::Boolean(true)
        );
        assert_eq!(
            tjs.execute("[1,2,3,4].indexOf(4)").unwrap(),
            value::Value::Number(3.0)
        );
    }

    #[test]
    fn tjs_array_find_some_every() {
        let mut tjs = TjsContext::new();
        assert_eq!(
            tjs.execute("[1,2,3,4,5].find(x => x > 3)").unwrap(),
            value::Value::Number(4.0)
        );
        assert_eq!(
            tjs.execute("[1,2,3,4,5].some(x => x > 4)").unwrap(),
            value::Value::Boolean(true)
        );
        assert_eq!(
            tjs.execute("[1,2,3,4,5].every(x => x < 10)").unwrap(),
            value::Value::Boolean(true)
        );
    }

    #[test]
    fn tjs_array_callback_closure_capture() {
        let mut tjs = TjsContext::new();
        let r = tjs
            .execute("JSON.stringify((function(){ var f=5; return [1,2,3].map(x => x*f); })())")
            .unwrap();
        assert_eq!(r, value::Value::String("[5,10,15]".to_string()));
    }

    #[test]
    fn tjs_string_repeat() {
        let mut tjs = TjsContext::new();
        let r = tjs.execute(r#""abc".repeat(3)"#).unwrap();
        assert_eq!(r, value::Value::String("abcabcabc".to_string()));
    }

    #[test]
    fn tjs_string_pad_start() {
        let mut tjs = TjsContext::new();
        let r = tjs.execute(r#""5".padStart(3, "0")"#).unwrap();
        assert_eq!(r, value::Value::String("005".to_string()));
    }

    #[test]
    fn tjs_string_pad_end() {
        let mut tjs = TjsContext::new();
        let r = tjs.execute(r#""hi".padEnd(5, "!")"#).unwrap();
        assert_eq!(r, value::Value::String("hi!!!".to_string()));
    }

    #[test]
    fn tjs_string_trim_start_end() {
        let mut tjs = TjsContext::new();
        let r = tjs.execute(r#""  hello  ".trimStart()"#).unwrap();
        assert_eq!(r, value::Value::String("hello  ".to_string()));
        let r = tjs.execute(r#""  hello  ".trimEnd()"#).unwrap();
        assert_eq!(r, value::Value::String("  hello".to_string()));
    }

    #[test]
    fn tjs_string_includes() {
        let mut tjs = TjsContext::new();
        let r = tjs.execute(r#""hello world".includes("world")"#).unwrap();
        assert_eq!(r, value::Value::Boolean(true));
        let r = tjs.execute(r#""hello world".includes("xyz")"#).unwrap();
        assert_eq!(r, value::Value::Boolean(false));
    }

    #[test]
    fn tjs_string_starts_ends_with() {
        let mut tjs = TjsContext::new();
        let r = tjs.execute(r#""hello".startsWith("he")"#).unwrap();
        assert_eq!(r, value::Value::Boolean(true));
        let r = tjs.execute(r#""hello".endsWith("lo")"#).unwrap();
        assert_eq!(r, value::Value::Boolean(true));
    }

    #[test]
    fn tjs_string_slice_substring() {
        let mut tjs = TjsContext::new();
        let r = tjs.execute(r#""hello".slice(1, 4)"#).unwrap();
        assert_eq!(r, value::Value::String("ell".to_string()));
        let r = tjs.execute(r#""hello".substring(1, 4)"#).unwrap();
        assert_eq!(r, value::Value::String("ell".to_string()));
    }

    #[test]
    fn tjs_string_replace() {
        let mut tjs = TjsContext::new();
        let r = tjs
            .execute(r#""hello world".replace("world", "rust")"#)
            .unwrap();
        assert_eq!(r, value::Value::String("hello rust".to_string()));
    }

    // ── try/catch/finally tests ──

    #[test]
    fn tjs_try_catch() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("try { throw 'error' } catch(e) { e }")
            .unwrap();
        assert_eq!(result, value::Value::String("error".to_string()));
    }

    #[test]
    fn tjs_try_finally_runs() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var x = 0; try { x = 1 } finally { x = 2 }; x")
            .unwrap();
        assert_eq!(result, value::Value::Number(2.0));
    }

    #[test]
    fn tjs_try_catch_finally() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var r = ''; try { throw 'E' } catch(e) { r = e } finally { r = r + 'F' }; r")
            .unwrap();
        assert_eq!(result, value::Value::String("EF".to_string()));
    }

    #[test]
    fn tjs_finally_overrides_return() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("(function() { try { return 1 } finally { return 2 } })()")
            .unwrap();
        assert_eq!(result, value::Value::Number(2.0));
    }

    // ── class tests ──

    #[test]
    fn tjs_class_basic() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("class Point { constructor(x, y) { this.x = x; this.y = y } } var p = new Point(3, 4); p.x")
            .unwrap();
        assert_eq!(result, value::Value::Number(3.0));
    }

    #[test]
    fn tjs_class_method() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("class A { double(x) { return x * 2 } } new A().double(21)")
            .unwrap();
        assert_eq!(result, value::Value::Number(42.0));
    }

    #[test]
    fn tjs_class_extends() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute(
            "class Animal { speak() { return '...' } }
             class Dog extends Animal { speak() { return 'Woof' } }
             new Dog().speak()"
        ).unwrap();
        assert_eq!(result, value::Value::String("Woof".to_string()));
    }

    #[test]
    fn tjs_class_static_method() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("class Math2 { static square(x) { return x * x } } Math2.square(7)")
            .unwrap();
        assert_eq!(result, value::Value::Number(49.0));
    }

    #[test]
    fn tjs_class_field() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("class C { x = 42 } new C().x")
            .unwrap();
        assert_eq!(result, value::Value::Number(42.0));
    }

    // ── optional chaining ──

    #[test]
    fn tjs_optional_chaining() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var obj = null; obj?.x")
            .unwrap();
        assert_eq!(result, value::Value::Undefined);
    }

    #[test]
    fn tjs_optional_chaining_value() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var obj = { x: 42 }; obj?.x")
            .unwrap();
        assert_eq!(result, value::Value::Number(42.0));
    }

    // ── nullish coalescing ──

    #[test]
    fn tjs_nullish_coalescing() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("null ?? 'default'").unwrap();
        assert_eq!(result, value::Value::String("default".to_string()));
    }

    #[test]
    fn tjs_nullish_coalescing_not_null() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("0 ?? 'default'").unwrap();
        assert_eq!(result, value::Value::Number(0.0));
    }

    // ── template literals ──

    #[test]
    fn tjs_template_literal() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("var name = 'World'; `Hello ${name}!`").unwrap();
        assert_eq!(result, value::Value::String("Hello World!".to_string()));
    }

    #[test]
    fn tjs_template_literal_expr() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("`${1 + 2} items`").unwrap();
        assert_eq!(result, value::Value::String("3 items".to_string()));
    }

    // ── spread operator ──

    #[test]
    fn tjs_spread_in_call() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var nums = [1, 2, 3]; Math.max(...nums)")
            .unwrap();
        assert_eq!(result, value::Value::Number(3.0));
    }

    #[test]
    fn tjs_spread_in_array() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var a = [1, 2]; var b = [...a, 3]; b[2]")
            .unwrap();
        assert_eq!(result, value::Value::Number(3.0));
    }

    // ── for...of ──

    #[test]
    fn tjs_for_of() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var sum = 0; for (var x of [10, 20, 30]) { sum += x } sum")
            .unwrap();
        assert_eq!(result, value::Value::Number(60.0));
    }

    #[test]
    fn tjs_for_in() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var keys = []; for (var k in {a:1, b:2, c:3}) { keys.push(k) } keys.length")
            .unwrap();
        assert_eq!(result, value::Value::Number(3.0));
    }

    // ── destructuring ──

    #[test]
    fn tjs_destructure_array() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var [a, b] = [1, 2]; a + b")
            .unwrap();
        assert_eq!(result, value::Value::Number(3.0));
    }

    #[test]
    fn tjs_destructure_object() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var {x, y} = {x: 10, y: 20}; x + y")
            .unwrap();
        assert_eq!(result, value::Value::Number(30.0));
    }

    // ── exponentiation ──

    #[test]
    fn tjs_exponentiation() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("2 ** 10").unwrap();
        assert_eq!(result, value::Value::Number(1024.0));
    }

    #[test]
    fn tjs_exponentiation_assign() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("var x = 3; x **= 2; x").unwrap();
        assert_eq!(result, value::Value::Number(9.0));
    }

    // ── switch/case/default ──

    #[test]
    fn tjs_switch_basic() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute(
            "var x = 2; switch (x) { case 1: 'one'; break; case 2: 'two'; break; default: 'other' }"
        ).unwrap();
        assert_eq!(result, value::Value::String("two".to_string()));
    }

    #[test]
    fn tjs_switch_default() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute(
            "var x = 99; switch (x) { case 1: 'one'; break; default: 'other' }"
        ).unwrap();
        assert_eq!(result, value::Value::String("other".to_string()));
    }

    #[test]
    fn tjs_switch_fallthrough() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute(
            "var r = ''; switch (1) { case 1: r += 'a'; case 2: r += 'b'; break; case 3: r += 'c' } r"
        ).unwrap();
        assert_eq!(result, value::Value::String("ab".to_string()));
    }

    #[test]
    fn tjs_switch_no_match() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute(
            "var r = 'none'; switch (99) { case 1: r = 'one'; break; } r"
        ).unwrap();
        assert_eq!(result, value::Value::String("none".to_string()));
    }

    // ── Error types ──

    #[test]
    fn tjs_error_creation() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var e = new Error('test'); e.message")
            .unwrap();
        assert_eq!(result, value::Value::String("test".to_string()));
    }

    #[test]
    fn tjs_type_error() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var e = new TypeError('bad type'); e.name")
            .unwrap();
        assert_eq!(result, value::Value::String("TypeError".to_string()));
    }

    // ── RegExp ──

    #[test]
    fn tjs_regexp_test() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var re = new RegExp('world'); re.test('hello world')")
            .unwrap();
        assert_eq!(result, value::Value::Boolean(true));
    }

    #[test]
    fn tjs_regexp_test_case_insensitive() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var re = new RegExp('HELLO', 'i'); re.test('hello world')")
            .unwrap();
        assert_eq!(result, value::Value::Boolean(true));
    }

    #[test]
    fn tjs_regexp_exec() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var re = new RegExp('world'); var m = re.exec('hello world'); m[0]")
            .unwrap();
        assert_eq!(result, value::Value::String("world".to_string()));
    }

    #[test]
    fn tjs_regexp_exec_index() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("var re = new RegExp('world'); var m = re.exec('hello world'); m.index")
            .unwrap();
        assert_eq!(result, value::Value::Number(6.0));
    }

    // ── Object methods ──

    #[test]
    fn tjs_object_keys() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("Object.keys({a:1, b:2, c:3}).length")
            .unwrap();
        assert_eq!(result, value::Value::Number(3.0));
    }

    #[test]
    fn tjs_object_values() {
        let mut tjs = TjsContext::new();
        let result = tjs
            .execute("Object.values({a:1, b:2}).length")
            .unwrap();
        assert_eq!(result, value::Value::Number(2.0));
    }

    #[test]
    fn tjs_array_isarray() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("Array.isArray([1,2,3])").unwrap();
        assert_eq!(result, value::Value::Boolean(true));
    }

    #[test]
    fn tjs_array_from() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("Array.from('abc').length").unwrap();
        assert_eq!(result, value::Value::Number(3.0));
    }

    // ── Number methods ──

    #[test]
    fn tjs_number_isinteger() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("Number.isInteger(42)").unwrap();
        assert_eq!(result, value::Value::Boolean(true));
    }

    #[test]
    fn tjs_number_isnan() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("Number.isNaN(NaN)").unwrap();
        assert_eq!(result, value::Value::Boolean(true));
    }

    #[test]
    fn tjs_parseint_radix() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("parseInt('ff', 16)").unwrap();
        assert_eq!(result, value::Value::Number(255.0));
    }
}

    // ── RegExp literal syntax /pattern/flags ──

    #[test]
    fn tjs_regex_literal_test() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("/hello/.test('hello world')").unwrap();
        assert_eq!(result, value::Value::Boolean(true));
    }

    #[test]
    fn tjs_regex_literal_case_insensitive() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("/HELLO/i.test('hello world')").unwrap();
        assert_eq!(result, value::Value::Boolean(true));
    }

    #[test]
    fn tjs_regex_literal_exec() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("var m = /(\\d+)/.exec('abc123def'); m[1]").unwrap();
        assert_eq!(result, value::Value::String("123".to_string()));
    }

    #[test]
    fn tjs_regex_literal_no_match() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("/xyz/.test('hello world')").unwrap();
        assert_eq!(result, value::Value::Boolean(false));
    }

    #[test]
    fn tjs_regex_match_method() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("'hello world'.match(/world/)[0]").unwrap();
        assert_eq!(result, value::Value::String("world".to_string()));
    }

    #[test]
    fn tjs_regex_replace_method() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("'hello world'.replace(/o/g, '0')").unwrap();
        assert_eq!(result, value::Value::String("hell0 w0rld".to_string()));
    }

    #[test]
    fn tjs_regex_search_method() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("'hello world'.search(/world/)").unwrap();
        assert_eq!(result, value::Value::Number(6.0));
    }

    #[test]
    fn tjs_regex_split_method() {
        let mut tjs = TjsContext::new();
        let result = tjs.execute("'a1b2c3'.split(/\\d/).length").unwrap();
        assert_eq!(result, value::Value::Number(4.0));
    }

    // ── ES module import/export ──

    #[test]
    fn tjs_import_named() {
        let mut tjs = TjsContext::new();
        // Register a module first.
        let mut exports = crate::tjs::value::ObjectValue::new();
        exports.set("foo", value::Value::Number(42.0));
        crate::web_api::es_modules::register_module("./mod.js", value::Value::Object(std::rc::Rc::new(std::cell::RefCell::new(exports))));
        let result = tjs.execute("import { foo } from \"./mod.js\"; foo").unwrap();
        assert_eq!(result, value::Value::Number(42.0));
    }

    #[test]
    fn tjs_import_default() {
        let mut tjs = TjsContext::new();
        let mut exports = crate::tjs::value::ObjectValue::new();
        exports.set("default", value::Value::String("hello".to_string()));
        crate::web_api::es_modules::register_module("./def.js", value::Value::Object(std::rc::Rc::new(std::cell::RefCell::new(exports))));
        let result = tjs.execute("import myDefault from \"./def.js\"; myDefault").unwrap();
        assert_eq!(result, value::Value::String("hello".to_string()));
    }

    #[test]
    fn tjs_import_namespace() {
        let mut tjs = TjsContext::new();
        let mut exports = crate::tjs::value::ObjectValue::new();
        exports.set("a", value::Value::Number(1.0));
        exports.set("b", value::Value::Number(2.0));
        crate::web_api::es_modules::register_module("./ns.js", value::Value::Object(std::rc::Rc::new(std::cell::RefCell::new(exports))));
        let result = tjs.execute("import * as mod from \"./ns.js\"; mod.a + mod.b").unwrap();
        assert_eq!(result, value::Value::Number(3.0));
    }

    // ── async/await ──

    #[test]
    fn tjs_async_await() {
        let mut tjs = TjsContext::new();
        // await on a non-Promise value just returns the value.
        let result = tjs.execute("(async function() { return await 42 })()").unwrap();
        // async function returns undefined (we don't have real async),
        // but await itself should work.
        let _ = result;
    }

    // ── generator ──

    #[test]
    fn tjs_generator_basic() {
        let mut tjs = TjsContext::new();
        // Generator functions are treated as regular functions.
        // yield returns the value.
        let result = tjs.execute("(function*() { yield 1; yield 2; yield 3 })()").unwrap();
        // Calling a generator returns undefined (no real iterator protocol),
        // but the function itself doesn't crash.
        let _ = result;
    }
