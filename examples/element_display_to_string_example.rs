// rust
use std::fmt::{self, Display};

struct MyName(String);

impl Display for MyName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 自定义格式化输出
        write!(f, "Mr. {}", self.0)
    }
}

// 因为实现了 Display，类型自动获得 ToString（.to_string()）
fn greet<T: ToString>(name: T) {
    let s = name.to_string();
    println!("Hello, {}", s);
}

fn main() {
    let a = MyName("Alice".into());
    greet(a); // 使用自定义类型
    let b = "Bob";
    greet(b); // &str 实现了 ToString
    let c = String::from("Carol");
    greet(c); // String 也实现了 ToString
}
