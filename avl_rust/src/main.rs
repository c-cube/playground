use std::ptr;

struct Node {
    k: i32,
    l: *mut Node,
    r: *mut Node,
    depth: u32,
}

pub struct Tree {
    root: *mut Node,
}

const NULL_NODE: *mut Node = ptr::null_mut();

fn new_node(k: i32) -> *mut Node {
    let node = Node {
        k,
        l: NULL_NODE,
        r: NULL_NODE,
        depth: 1,
    };
    let boxed = Box::new(node);
    Box::into_raw(boxed)
}

fn del_node(n: *mut Node) {
    let b = unsafe { Box::from_raw(n) };
    drop(b)
}

unsafe fn traverse_rec_(n: *mut Node, res: &mut Vec<i32>) {
    if !n.is_null() {
        let nref: &Node = &*n;
        traverse_rec_(nref.l, res);
        res.push(nref.k);
        traverse_rec_(nref.r, res);
    }
}

unsafe fn depth_(n: *mut Node) -> u32 {
    match n.as_ref() {
        None => 0,
        Some(nr) => nr.depth,
    }
}

fn update_depth(n: &mut Node) {
    n.depth = unsafe { depth_(n.l).max(depth_(n.r)) + 1 }
}

unsafe fn rebalance(n: *mut Node) -> *mut Node {
    debug_assert!(!n.is_null());
    let nref = &mut *n;
    let depth_l = depth_(nref.l);
    let depth_r = depth_(nref.r);
    if depth_l + 1 < depth_r {
        // rotate to make r the root with n as its left child
        let r = &mut *nref.r;
        nref.r = r.l;
        update_depth(nref);
        r.l = n;
        update_depth(r);
        r as &mut _ as *mut _
    } else if depth_l > depth_r + 1 {
        // rotate
        let l = &mut *nref.l;
        nref.l = l.r;
        update_depth(nref);
        l.r = nref;
        update_depth(l);
        l as &mut _ as *mut _
    } else {
        update_depth(nref);
        n
    }
}

// insert as if it were a linked list
unsafe fn add_rec_(n: *mut Node, k: i32) -> *mut Node {
    match n.as_mut() {
        Some(nref) if nref.k == k => n,
        Some(nref) => {
            if k < nref.k {
                // insert left
                let l = add_rec_(nref.l, k);
                nref.l = l;
            } else {
                // insert right
                let r = add_rec_(nref.r, k);
                nref.r = r;
            }
            rebalance(n)
        }
        None => {
            let n2 = new_node(k);
            n2
        }
    }
}

unsafe fn delete_rec(n: *mut Node) -> *mut Node {
    if !n.is_null() {
        {
            let nref: &mut Node = &mut *n;
            nref.l = delete_rec(nref.l);
            nref.r = delete_rec(nref.r);
        }
        del_node(n);
    }
    ptr::null_mut()
}

impl Tree {
    pub fn new() -> Self {
        Tree { root: NULL_NODE }
    }

    /// Traverse the tree to get a Vec
    pub fn traverse(&self) -> Vec<i32> {
        let mut res = vec![];
        unsafe { traverse_rec_(self.root, &mut res) };
        res
    }

    /// Add a value
    pub fn add(&mut self, k: i32) {
        unsafe {
            let n = add_rec_(self.root, k);
            self.root = n
        }
    }

    /// Tree containing 0..upto
    pub fn range(upto: i32) -> Self {
        let mut tree = Tree::new();
        for i in (0..upto as usize).rev() {
            tree.add(i as i32)
        }
        tree
    }

    pub fn depth(&self) -> u32 {
        unsafe { depth_(self.root) }
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        self.root = unsafe { delete_rec(self.root) }
    }
}

fn sorted(r: &[i32]) -> bool {
    if r.len() <= 1 {
        return true;
    }
    for i in 1..r.len() {
        if r[i] < r[i - 1] {
            return false;
        }
    }
    true
}

fn main() {
    let t = Tree::range(5_000_000);
    println!("depth: {}", t.depth());
    let v = t.traverse();
    println!("v.len: {:?}", v.len());
    dbg!(sorted(&v));
}
