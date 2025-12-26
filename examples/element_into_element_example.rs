// rust
use std::any::Any;

// 静态核心 trait：只定义一个简单的 render 方法
pub trait Element: 'static + IntoElement {
    fn render(&self) -> String;

    // 提供一个方便方法把自己装箱成 AnyElement
    fn into_any(self) -> AnyElement
    where
        Self: Sized,
    {
        AnyElement::new(self)
    }
}

// 转换 trait：任何能转成 Element 的类型都实现它
pub trait IntoElement: Sized {
    type Element: Element;
    fn into_element(self) -> Self::Element;

    fn into_any_element(self) -> AnyElement {
        self.into_element().into_any()
    }
}

// 运行时 trait-object 接口
trait ElementObject {
    fn render(&self) -> String;
    fn as_any(&self) -> &dyn Any;
}

// 把任意实现了 Element 的类型封装为 Drawable，以实现 ElementObject
struct Drawable<E: Element> {
    element: E,
}

impl<E: Element> ElementObject for Drawable<E> {
    fn render(&self) -> String {
        self.element.render()
    }
    fn as_any(&self) -> &dyn Any {
        &self.element
    }
}

// AnyElement：运行时容器，持有 Box<dyn ElementObject>
pub struct AnyElement(Box<dyn ElementObject>);

impl AnyElement {
    pub fn new<E: Element>(element: E) -> Self {
        AnyElement(Box::new(Drawable { element }))
    }

    pub fn render(&self) -> String {
        self.0.render()
    }

    // 向下转换为具体类型的引用（如果可能）
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.0.as_any().downcast_ref::<T>()
    }
}

// 示例具体类型：Label
pub struct Label(pub String);

// Label 可以直接作为 Element（实现生命周期方法）
impl Element for Label {
    fn render(&self) -> String {
        format!("Label: {}", self.0)
    }
}

// 以及实现 IntoElement，使得 Label 类型自身可用作 into_element
impl IntoElement for Label {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

// 允许从 String 直接转换为 Label（调用方可以传 String）
impl IntoElement for String {
    type Element = Label;
    fn into_element(self) -> Label {
        Label(self)
    }
}

fn main() {
    // 使用静态类型并装箱到 AnyElement
    let a = Label("Alice".into()).into_any();
    let b = String::from("Bob").into_any_element(); // 通过 IntoElement for String

    let elements: Vec<AnyElement> = vec![a, b];

    for e in &elements {
        println!("{}", e.render());
    }

    // 尝试向下转换回 Label
    if let Some(label) = elements[0].downcast_ref::<Label>() {
        println!("downcasted: {}", label.0);
    }
}
