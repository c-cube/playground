use std::{
    collections::HashMap,
    fmt::Debug,
    ptr::NonNull,
    sync::atomic::{AtomicU32, Ordering},
};

use smallvec::SmallVec;
pub type Name = Box<str>;

#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum Tag {
    Var,
    BVar,
    Const,
    App,
    AppBin,
    Bind,
}

#[derive(Clone, Debug)]
pub enum TermRef<'a> {
    Var(&'a Var),
    BVar(&'a BVar),
    Const(&'a Const),
    App(&'a App),
    AppBin(&'a AppBin),
    Bind(&'a Bind),
}

#[derive(Clone, Debug)]
pub struct Var {
    pub idx: u32,
    pub ty: Term,
}

#[derive(Clone, Debug)]
pub struct BVar {
    pub dbidx: u32,
}

#[derive(Clone, Debug)]
pub struct Const {
    pub name: Name,
}

#[derive(Clone, Debug)]
pub struct App {
    pub f: Term,
    pub args: SmallVec<[Term; 3]>,
}

#[derive(Clone, Debug)]
pub struct AppBin {
    pub f: Term,
    pub arg: Term,
}

#[derive(Clone, Debug)]
pub struct Bind {
    pub binder: Term,
    pub var: Term,
    pub body: Term,
}

/*
macro_rules! tag2enum {
    (name: $ident, tag:$ident) => {
        fn $name(x: $tag) ->

    }
}
*/

#[repr(C)]
pub struct TermView<T> {
    tag: Tag,
    rc: AtomicU32,
    value: T,
}

#[derive(Eq, PartialEq, Hash)]
pub struct Term(NonNull<TermView<()>>);

impl Term {
    #[inline]
    #[allow(unsafe_code)]
    pub fn tag(&self) -> Tag {
        unsafe { self.0.as_ref() }.tag
    }
}

macro_rules! cast_to {
    ($t: expr, $tag: expr, $ty: ty) => {{
        let t2: &TermView<()> = $t.0.as_ref();
        debug_assert_eq!(t2.tag, $tag);
        let new_ptr: NonNull<TermView<$ty>> = $t.0.cast::<TermView<$ty>>();
        new_ptr
    }};
}

macro_rules! builder {
    ($mk: ident, $tag: expr, $ty: ty) => {
        #[allow(unsafe_code)]
        pub fn $mk(x: $ty) -> Term {
            let ptr_box = Box::new(TermView {
                tag: $tag,
                rc: AtomicU32::new(1),
                value: x,
            });
            let ptr = unsafe { NonNull::new_unchecked(Box::into_raw(ptr_box)) };
            let ptr_erased = unsafe { ptr.cast::<TermView<()>>() };
            Term(ptr_erased)
        }
    };
}

macro_rules! get_case {
    ($f: ident, $tag: expr, $ty: ty) => {
        #[inline]
        #[allow(unsafe_code)]
        pub fn $f(self: &Term) -> &$ty {
            debug_assert_eq!(self.tag(), $tag);
            let new_ptr: NonNull<TermView<$ty>> = self.0.cast::<TermView<$ty>>();
            let ref_to_ty: &TermView<$ty> = unsafe { new_ptr.as_ref() };
            &ref_to_ty.value
        }
    };
}

macro_rules! define_cstor {
    ($mk_builder: ident, $get_case: ident, $tag: expr, $ty: ty) => {
        builder!($mk_builder, $tag, $ty);
        get_case!($get_case, $tag, $ty);
    };
}

impl Term {
    define_cstor!(mk_var, as_var, Tag::Var, Var);
    define_cstor!(mk_bvar, as_bvar, Tag::BVar, BVar);
    define_cstor!(mk_const, as_const, Tag::Const, Const);
    define_cstor!(mk_app, as_app, Tag::App, App);
    define_cstor!(mk_app_bin, as_app_bin, Tag::AppBin, AppBin);
    define_cstor!(mk_bind, as_bind, Tag::Bind, Bind);

    #[allow(unsafe_code)]
    pub fn view(self: &Term) -> TermRef {
        let tag = self.tag();
        match tag {
            Tag::Var => TermRef::Var(self.as_var()),
            Tag::BVar => TermRef::BVar(self.as_bvar()),
            Tag::Const => TermRef::Const(self.as_const()),
            Tag::App => TermRef::App(self.as_app()),
            Tag::AppBin => TermRef::AppBin(self.as_app_bin()),
            Tag::Bind => TermRef::Bind(self.as_bind()),
        }
    }
}

impl Clone for Term {
    #[allow(unsafe_code)]
    fn clone(&self) -> Self {
        let view: &TermView<()> = unsafe { self.0.as_ref() };
        view.rc.fetch_add(1, Ordering::AcqRel);
        Term(self.0)
    }
}

/// Actually drop the term
#[allow(unsafe_code)]
fn drop_inside(t: &mut Term) {
    let tag: Tag = t.tag();
    unsafe {
        match tag {
            Tag::Var => {
                drop(Box::from_raw(cast_to!(t, tag, Var).as_ptr()));
            }
            Tag::BVar => {
                drop(Box::from_raw(cast_to!(t, tag, BVar).as_ptr()));
            }
            Tag::Const => {
                drop(Box::from_raw(cast_to!(t, tag, Const).as_ptr()));
            }
            Tag::App => {
                drop(Box::from_raw(cast_to!(t, tag, App).as_ptr()));
            }
            Tag::AppBin => {
                drop(Box::from_raw(cast_to!(t, tag, AppBin).as_ptr()));
            }
            Tag::Bind => {
                drop(Box::from_raw(cast_to!(t, tag, Bind).as_ptr()));
            }
        }
    }
}

impl Drop for Term {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        let view: &TermView<()> = unsafe { self.0.as_ref() };
        if view.rc.fetch_sub(1, Ordering::AcqRel) == 1 {
            // time to drop the view
            drop_inside(self);
        }
    }
}

impl Debug for Term {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.view() {
            TermRef::Var(v) => write!(f, "?{}", v.idx),
            TermRef::BVar(v) => write!(f, "bvar{}", v.dbidx),
            TermRef::Const(c) => write!(f, "{}", c.name),
            TermRef::App(a) => write!(f, "({:?} {:?})", a.f, a.args),
            TermRef::AppBin(a) => write!(f, "({:?} {:?})", a.f, a.arg),
            TermRef::Bind(b) => {
                write!(f, "bind({:?}, {:?}, {:?}", b.binder, b.var, b.body)
            }
        }
    }
}

impl Term {
    pub fn size_tree(self: &Term) -> usize {
        let mut size = 0;
        let mut stack = vec![self];

        while let Some(t) = stack.pop() {
            size += 1;
            match t.view() {
                TermRef::Var(_) | TermRef::BVar(_) | TermRef::Const(_) => (),
                TermRef::AppBin(app) => {
                    stack.push(&app.f);
                    stack.push(&app.arg);
                }
                TermRef::App(app) => {
                    stack.push(&app.f);
                    for a in &app.args {
                        stack.push(a)
                    }
                }
                TermRef::Bind(bind) => {
                    stack.push(&bind.binder);
                    stack.push(&bind.var);
                    stack.push(&bind.body);
                }
            }
        }
        size
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn size_term() {
        assert_eq!(8, std::mem::size_of::<Term>())
    }

    #[test]
    fn build_some() {
        let f = Term::mk_const(Const {
            name: "f".to_string().into_boxed_str(),
        });
        let a = Term::mk_const(Const {
            name: "a".to_string().into_boxed_str(),
        });
        let tau = Term::mk_const(Const {
            name: "tau".to_string().into_boxed_str(),
        });

        let x = Term::mk_var(Var {
            idx: 0,
            ty: tau.clone(),
        });
        let t = Term::mk_app_bin(AppBin {
            f: f.clone(),
            arg: Term::mk_app(App {
                f: f.clone(),
                args: smallvec::smallvec![a.clone(), x.clone()],
            }),
        });

        eprintln!("t: {:?}", t)
    }

    #[test]
    fn build_many() {
        let f = Term::mk_const(Const {
            name: "f".to_string().into_boxed_str(),
        });
        let a = Term::mk_const(Const {
            name: "a".to_string().into_boxed_str(),
        });
        let tau = Term::mk_const(Const {
            name: "tau".to_string().into_boxed_str(),
        });

        for _i in 0..20 {
            let mut t = a.clone();
            for i in 0..100 {
                t = Term::mk_app(App {
                    f: f.clone(),
                    args: smallvec::smallvec![t.clone(), t.clone(), t.clone()],
                })
            }
            // TODO: how do we compute this??
            // eprintln!("term.size={}", t.size_tree())
        }
    }
}
