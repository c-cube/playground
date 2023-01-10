use avl_test::*;

fn main() {
    let t = Tree::range(5_000_000);
    println!("depth: {}", t.depth());
    let v = t.traverse();
    println!("v.len: {:?}", v.len());
    dbg!(sorted(&v));
}
