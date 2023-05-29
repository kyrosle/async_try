use std::{
  cell::RefCell,
  future::Future,
  task::{Context, Poll, Waker},
  thread,
};

use crossbeam::sync::Parker;

/// v1
fn block_on_v1<F: Future>(future: F) -> F::Output {
  // pin future in stack
  pin_utils::pin_mut!(future);

  let thread = thread::current();
  // callback function
  let waker = async_task::waker_fn(move || thread.unpark());

  let cx = &mut Context::from_waker(&waker);
  loop {
    match future.as_mut().poll(cx) {
      Poll::Ready(output) => return output,
      Poll::Pending => thread::park(),
    }
  }
}

/// v2 Causes it to be woken up when it should not be woken up,
/// or does not receive a wake-up notification.
fn block_on_v2<F: Future>(future: F) -> F::Output {
  // pin future in stack
  pin_utils::pin_mut!(future);

  let parker = Parker::new();
  let unparker = parker.unparker().clone();
  // callback function
  let waker = async_task::waker_fn(move || unparker.unpark());

  let cx = &mut Context::from_waker(&waker);
  loop {
    match future.as_mut().poll(cx) {
      Poll::Ready(output) => return output,
      Poll::Pending => thread::park(),
    }
  }
}

/// v3 store `Parker` and `Waker` in thread cache
fn block_on_v3<F: Future>(future: F) -> F::Output {
  pin_utils::pin_mut!(future);

  thread_local! {
    static CACHE: (Parker, Waker) = {
      let parker = Parker::new();
      let unparker = parker.unparker().clone();
      let waker = async_task::waker_fn(move || unparker.unpark());
      (parker, waker)
    };
  }

  CACHE.with(|(parker, waker)| {
    let cx = &mut Context::from_waker(&waker);
    loop {
      match future.as_mut().poll(cx) {
        Poll::Ready(output) => return output,
        Poll::Pending => parker.park(),
      }
    }
  })
}

/// v4 avoid recursive call
pub fn block_on_v4<F: Future>(future: F) -> F::Output {
  pin_utils::pin_mut!(future);

  thread_local! {
    static CACHE: RefCell<(Parker, Waker) >= {
      let parker = Parker::new();
      let unparker = parker.unparker().clone();
      let waker = async_task::waker_fn(move || unparker.unpark());
      RefCell::new((parker, waker))
    };
  }

  CACHE.with(|cache| {
    let (parker, waker) =
      &mut *cache.try_borrow_mut().expect("recursive `block_on`");

    let cx = &mut Context::from_waker(&waker);
    loop {
      match future.as_mut().poll(cx) {
        Poll::Ready(output) => return output,
        Poll::Pending => parker.park(),
      }
    }
  })
}

struct Yields(u32);

impl Future for Yields {
  type Output = ();

  fn poll(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Self::Output> {
    if self.0 == 0 {
      Poll::Ready(())
    } else {
      self.0 -= 1;
      cx.waker().wake_by_ref();
      Poll::Pending
    }
  }
}