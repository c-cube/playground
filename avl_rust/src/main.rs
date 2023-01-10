
use avl_test::*;

#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    let t = Tree::range(5_000_000);
    //let t = Tree::range(500);
    //println!("tree: {}", t.print());
    println!("depth: {}", t.depth());
    let v = t.traverse();
    println!("v.len: {:?}", v.len());
    dbg!(sorted(&v));
    dbg!(avl_test::n_rebalance());
}
