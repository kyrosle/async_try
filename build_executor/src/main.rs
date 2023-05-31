use std::time::Duration;

use build_executor::{executor::new_executor_and_spawner, timer::TimerFuture};

fn main() {
  let (executor, spawner) = new_executor_and_spawner();

  spawner.spawn(async {
    println!("howdy1!");
    TimerFuture::new(Duration::new(2, 0)).await;
    println!("done1!");
  });
  spawner.spawn(async {
    println!("howdy2!");
    TimerFuture::new(Duration::new(5, 0)).await;
    println!("done2!");
  });

  drop(spawner);

  executor.run();
}
