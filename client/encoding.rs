// escape sequence for mouse event
const MOUSE_EVENT: &str = "\x1B[<";

// pub fn mouse_event(event: MouseEvent) -> String {
//     let MouseEvent { x, y, button, kind } = event;
//     format!("{}{}{}{}", MOUSE_EVENT, x, y, button, kind)
// }
