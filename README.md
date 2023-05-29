# Learn async in rust

## Futures Explained in 200 Lines of Rust

- <https://stevenbai.top/rust/futures_explained_in_200_lines_of_rust/>
- <https://stevenbai.top/rust/futures_explained_in_200_lines_of_rust2/>
- <https://zhuanlan.zhihu.com/p/129273132?utm_source=wechat_session&utm_medium=social&utm_oi=51691969839104>

## Build your own block_on()

block_on code:

```rust
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
```

test code:

```rust
struct Yields(u32);

impl Future for Yields {
  type Output = ();

  fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
    if self.0 == 0 {
      Poll::Ready(())
    } else {
      self.0 -= 1;
      cx.waker().wake_by_ref();
      Poll::Pending
    }
  }
}

fn custom_block_on_yields(yields_time: u32) {
  block_on_v4(Yields(yields_time));
}

fn futures_block_on_yields(yields_time: u32) {
  block_on(Yields(yields_time));
}
```

benchmark:

```rust
Gnuplot not found, using plotters backend
custom_block_on 10 yields
                        time:   [127.57 ns 128.16 ns 128.83 ns]
Found 6 outliers among 100 measurements (6.00%)
  3 (3.00%) high mild
  3 (3.00%) high severe

custom_block_on 20 yields
                        time:   [252.53 ns 253.68 ns 255.12 ns]
Found 7 outliers among 100 measurements (7.00%)
  4 (4.00%) high mild
  3 (3.00%) high severe

custom_block_on 30 yields
                        time:   [376.81 ns 378.01 ns 379.31 ns]
Found 7 outliers among 100 measurements (7.00%)
  1 (1.00%) high mild
  6 (6.00%) high severe

futures_block_on 10 yields
                        time:   [164.82 ns 165.49 ns 166.20 ns]
Found 7 outliers among 100 measurements (7.00%)
  3 (3.00%) high mild
  4 (4.00%) high severe

futures_block_on 20 yields
                        time:   [332.11 ns 335.10 ns 338.08 ns]
Found 8 outliers among 100 measurements (8.00%)
  6 (6.00%) high mild
  2 (2.00%) high severe

futures_block_on 30 yields
                        time:   [498.61 ns 501.92 ns 505.08 ns]
Found 3 outliers among 100 measurements (3.00%)
  1 (1.00%) high mild
  2 (2.00%) high severe
```
