use cursive::Cursive;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub struct Callback<Args, Ret> {
    callback: Rc<RefCell<Option<Box<dyn Fn(&mut Cursive, Args) -> Ret>>>>,
}

impl<Args, Ret> Callback<Args, Ret> {
    pub fn new() -> Self {
        let callback = Rc::new(RefCell::new(None));
        Self { callback }
    }

    pub fn set<F>(&mut self, callback: F)
    where
        F: Fn(&mut Cursive, Args) -> Ret + 'static,
    {
        self.callback.replace(Some(Box::new(callback)));
    }

    pub fn call(&self, siv: &mut Cursive, args: Args) -> Option<Ret> {
        let callback = self.callback.borrow();
        match &*callback {
            None => None,
            Some(callback) => Some(callback(siv, args)),
        }
    }
}
