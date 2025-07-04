// use gpui::{
//     App, AppContext, //Container, Text, VerticalLayout, Scrollable,
// };

// pub struct ResponsePanel {
//     response_text: String,
// }

// impl ResponsePanel {
//     pub fn new() -> Self {
//         ResponsePanel {
//             response_text: String::new(),
//         }
//     }

//     pub fn set_response(&mut self, response: String) {
//         self.response_text = response;
//     }

//     pub fn view(&self, cx: &mut AppContext) {
//         Scrollable::new(cx)
//             .child(
//                 Container::new(cx)
//                     .layout(VerticalLayout::default())
//                     .child(Text::new(cx, &self.response_text)),
//             )
//             .build(cx);
//     }
// }
