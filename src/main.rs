use ui0::get_component;

// read component as arg and write to stdout
fn main() {
    for arg in std::env::args() {
        if let Some(component) = get_component(&arg) {
            println!("{}", component.source);
        }
    }
}
