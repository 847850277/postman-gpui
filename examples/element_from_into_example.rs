fn main() {
    // 直接使用 From
    let a = MyString::from("hello from");
    println!("a = {}", a.0);

    // 使用 Into（自动由 From 提供）
    let b: MyString = "hello into".into();
    //let b = "hello into".into(); 这种会报错，编译器无法推断类型
    println!("b = {}", b.0);

    let c: String = a.into(); // MyString -> String
    println!("c = {}", c);

    // Into 作为函数参数（可接受多种输入类型）
    greet("Alice");
    greet(String::from("Bob"));
}

// 自定义包装类型
struct MyString(String);

// 从 &str 转换为 MyString（实现 From）
impl From<&str> for MyString {
    fn from(s: &str) -> Self {
        MyString(s.to_string())
    }
}

// 从 MyString 转换回 String（也可以实现）
impl From<MyString> for String {
    fn from(ms: MyString) -> Self {
        ms.0
    }
}

// 接受任意能转为 String 的类型
fn greet(name: impl Into<String>) {
    let s: String = name.into();
    println!("Hello, {}!", s);
}
