use std::collections::{hash_map::Entry, HashMap};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Term(u32);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Level(u8);

pub const DUMMY_TERM: Term = Term(u32::MAX);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Const {
    idx: u32,
    ty: Term,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Var {
    pub idx: u32,
    pub ty: Term,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum TermKind {
    Type,
    Var(Var),
    Const(Const),
    App(Term, Term),
    Lam(Term, Term),
    Pi(Term, Term),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct TermView {
    kind: TermKind,
    ty: Term,
}

pub struct TermBank(Box<TermBankInner>);

pub struct TermBankInner {
    views: Vec<TermView>,
    hashcons: HashMap<TermView, Term>,

    // a few builtins
    t_type: Term,
    t_bool: Term,
    t_true: Term,
    t_false: Term,
}

impl TermBank {
    fn add_builtin_(&mut self, kind: TermKind, ty: Term) -> Term {
        let n = self.0.views.len();
        let tv = TermView { kind, ty };
        self.0.views.push(tv);
        let t = Term(n as u32);
        self.0.hashcons.insert(tv, t);
        t
    }

    fn add_term_(&mut self, kind: TermKind, ty: Term) -> Term {
        let tv = TermView { kind, ty };

        let TermBankInner {
            hashcons, views, ..
        } = &mut *self.0;

        match hashcons.entry(tv) {
            Entry::Occupied(t) => *t.get(),
            Entry::Vacant(e) => {
                let n = views.len();
                if n == u32::MAX as usize {
                    panic!("all terms have been allocated")
                }
                views.push(tv);
                let t = Term(n as u32);
                e.insert(t);
                t
            }
        }
    }

    pub fn new() -> Self {
        let mut tb = TermBank(Box::new(TermBankInner {
            views: vec![],
            hashcons: Default::default(),
            t_type: DUMMY_TERM,
            t_true: DUMMY_TERM,
            t_false: DUMMY_TERM,
            t_bool: DUMMY_TERM,
        }));

        tb.0.t_type = tb.add_builtin_(TermKind::Type, Term(0));
        tb.0.t_bool = tb.add_builtin_(
            TermKind::Const(Const {
                idx: 0, // FIXME: alloc const
                ty: tb.0.t_type,
            }),
            tb.0.t_type,
        );
        tb.0.t_true = tb.add_builtin_(
            TermKind::Const(Const {
                idx: 1, // FIXME: alloc const
                ty: tb.0.t_bool,
            }),
            tb.0.t_type,
        );
        tb.0.t_false = tb.add_builtin_(
            TermKind::Const(Const {
                idx: 2, // FIXME: alloc const
                ty: tb.0.t_bool,
            }),
            tb.0.t_type,
        );

        tb
    }

    #[inline(always)]
    pub fn bool_ty(&self) -> Term {
        self.0.t_bool
    }

    #[inline(always)]
    pub fn bool(&self, b: bool) -> Term {
        if b {
            self.0.t_true
        } else {
            self.0.t_false
        }
    }

    #[inline(always)]
    pub fn var(&mut self, v: Var) -> Term {
        self.add_term_(TermKind::Var(v), v.ty)
    }

    #[inline(always)]
    pub fn app(&mut self, f: Term, arg: Term) -> Term {
        // TODO: actually typecheck
        let ty = self.view(f).ty;
        self.add_term_(TermKind::App(f, arg), ty)
    }

    pub fn app_slice(&mut self, f: Term, args: &[Term]) -> Term {
        let mut t = f;
        for &a in args {
            t = self.app(t, a)
        }
        t
    }

    #[inline(always)]
    pub fn view(&self, t: Term) -> TermView {
        self.0.views[t.0 as usize]
    }

    pub fn iter_terms<'a>(&'a self) -> impl Iterator<Item = (Term, TermView)> + 'a {
        self.0
            .views
            .iter()
            .enumerate()
            .map(|(idx, tv)| (Term(idx as u32), *tv))
    }
}

fn main() {
    let mut tb = TermBank::new();

    let v1 = tb.var(Var {
        idx: 0,
        ty: tb.bool_ty(),
    });

    let _t = tb.app_slice(v1, &[tb.bool(true), tb.bool(false)]);

    let terms: Vec<_> = tb.iter_terms().collect();
    println!("v: {:?}", &terms);
}
