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