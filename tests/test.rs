fn top_level() {}

pub fn bar(x: i32) -> i32 {
    x + 1
}

async fn baz() -> Result<(), ()> {
    Ok(())
}

// Function returning a function
pub(crate) fn make_adder(y: i32) -> impl Fn(i32) -> i32 {
    move |x| x + y
}

// Function taking another function as argument
fn apply_twice<F>(f: F, x: i32) -> i32
where
    F: Fn(i32) -> i32,
{
    f(f(x))
}

// Function with a complex argument and return type
fn option_result_map(
    input: Option<Result<i32, &'static str>>,
) -> Result<Option<i32>, &'static str> {
    match input {
        Some(Ok(v)) => Ok(Some(v * 2)),
        Some(Err(e)) => Err(e),
        None => Ok(None),
    }
}

// Multiple return values via tuple
fn split_point(s: &str) -> (&str, &str) {
    let mid = s.len() / 2;
    s.split_at(mid)
}


pub mod my_mod {
    pub fn in_module() {}

    pub mod nest {
        pub fn in_nest_module() {}
    }
}


struct Foo;

impl Foo {
    fn method(&self) {}

    fn static_fn() -> i32 {
        42
    }

    fn returns_fn() -> fn(i32) -> i32 {
        |x| x * 2
    }

    fn takes_fn<F: Fn(i32) -> i32>(&self, f: F, v: i32) -> i32 {
        f(v)
    }
}


trait MyTrait {
    fn required(&self);

    fn provided() {
        println!("default");
    }
}

impl MyTrait for Foo {
    fn required(&self) {
        println!("Foo::required");
    }
}

fn generic_max<T: Ord>(a: T, b: T) -> T {
    if a >= b { a } else { b }
}

// Function returning Result with Box<dyn Trait>
fn boxed_trait_object(flag: bool) -> Result<Box<dyn MyTrait>, &'static str> {
    if flag {
        Ok(Box::new(Foo))
    } else {
        Err("not Foo")
    }
}

#[test]
fn my_test() { assert_eq!(2 + 2, 4); }

#[derive(Debug, Clone)]
struct Bar(i32);

fn nested_closures() -> impl Fn(i32) -> i32 {
    |x| {
        let inner = |y| move |z| x + y + z;
        inner(2)(3)
    }
}


fn main() {
    top_level();

    let f = Foo;
    f.method();
    Foo::static_fn();

    my_mod::in_module();
    my_mod::nest::in_nest_module();

    // Using higher-order functions
    let add5 = make_adder(5);
    println!("add5(10) = {}", add5(10));

    let doubled = apply_twice(|x| x + 1, 3);
    println!("apply_twice = {}", doubled);

    // Complex return types
    let val = Some(Ok(7));
    println!("option_result_map = {:?}", option_result_map(val));

    let (a, b) = split_point("abcdef");
    println!("split_point: {} | {}", a, b);

    // Functions from impl
    let double_fn = Foo::returns_fn();
    println!("double_fn(21) = {}", double_fn(21));

    let result = f.takes_fn(|x| x * x, 6);
    println!("takes_fn = {}", result);

    // Generics
    println!("generic_max = {}", generic_max(3, 5));

    // Trait default + impl
    f.required();
    Foo::provided();

    // Trait object
    if let Ok(obj) = boxed_trait_object(true) {
        obj.required();
    }
}
