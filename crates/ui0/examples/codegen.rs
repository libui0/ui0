use oxc_allocator::Allocator;
use ui0::Bundle;

fn main() {
    let allocator = Allocator::default();
    let source = "
function Component(props: Props) {
return (
<span>
<i>{props.first_name}</i>
<b>Hello, {props.middle_name}!</b>
Hi, {props.last_name}!
<Component>
<sup>Sup, {props.name}!</sup>
</Component>
</span>
);
}

";
    let mut c = Bundle::new(&allocator);
    c.add(source);

    println!("{:?}", c.js());
}
