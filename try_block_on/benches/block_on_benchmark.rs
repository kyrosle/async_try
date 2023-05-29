use criterion::{black_box, criterion_group, criterion_main, Criterion};
use futures::executor::block_on;
use std::{
  future::Future,
  pin::Pin,
  task::{Context, Poll},
};

use try_block_on::block_on_v4;

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

fn criterion_benchmark(c: &mut Criterion) {
  c.bench_function("custom_block_on 10 yields", |b| {
    b.iter(|| custom_block_on_yields(black_box(10)))
  });
  c.bench_function("custom_block_on 20 yields", |b| {
    b.iter(|| custom_block_on_yields(black_box(20)))
  });
  c.bench_function("custom_block_on 30 yields", |b| {
    b.iter(|| custom_block_on_yields(black_box(30)))
  });
  c.bench_function("futures_block_on 10 yields", |b| {
    b.iter(|| futures_block_on_yields(black_box(10)))
  });
  c.bench_function("futures_block_on 20 yields", |b| {
    b.iter(|| futures_block_on_yields(black_box(20)))
  });
  c.bench_function("futures_block_on 30 yields", |b| {
    b.iter(|| futures_block_on_yields(black_box(30)))
  });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
