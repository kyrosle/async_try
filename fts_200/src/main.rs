//! blog:
//! - https://stevenbai.top/rust/futures_explained_in_200_lines_of_rust/
//! - https://stevenbai.top/rust/futures_explained_in_200_lines_of_rust2/
//! - https://zhuanlan.zhihu.com/p/129273132?utm_source=wechat_session&utm_medium=social&utm_oi=51691969839104
use std::{
  collections::{hash_map, HashMap},
  future::Future,
  mem,
  pin::Pin,
  sync::{
    mpsc::{channel, Sender},
    Arc, Condvar, Mutex,
  },
  task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
  thread::{self, JoinHandle},
  time::{Duration, Instant},
};

fn main() {
  // timer recorder
  let start = Instant::now();
  // `Reactor`
  let reactor = Reactor::new();

  // asynchronous task 1
  let fut1 = async {
    let val = Task::new(reactor.clone(), 1, 1).await;
    println!("Got {} at time: {:.2}.", val, start.elapsed().as_secs_f32());
  };

  // asynchronous task 2
  let fut2 = async {
    let val = Task::new(reactor.clone(), 2, 2).await;
    println!("Got {} at time: {:.2}.", val, start.elapsed().as_secs_f32());
  };

  // main asynchronous task
  let mainfut = async {
    fut1.await;
    fut2.await;
  };

  // execute asynchronous task
  block_on(mainfut);
  // send close event through channel.
  reactor.lock().map(|mut r| r.close()).unwrap();
}
// ==== EXECUTOR ====
#[derive(Default)]
/// Custom hibernate (Mutex and Condvar)
struct Parker(Mutex<bool>, Condvar);

impl Parker {
  /// Sleep current thread, wake up by reactor
  fn park(&self) {
    let mut resumable = self.0.lock().unwrap();
    while !*resumable {
      resumable = self.1.wait(resumable).unwrap();
    }
    *resumable = false;
  }

  /// Wake up current thread, called by reactor
  fn unpark(&self) {
    *self.0.lock().unwrap() = true;
    self.1.notify_one();
  }
}

/// block current thread and execute async task, until the async task is finished.
fn block_on<F: Future>(mut future: F) -> F::Output {
  // waker role
  let parker = Arc::new(Parker::default());
  let mywaker = Arc::new(MyWaker {
    parker: parker.clone(),
  });
  let waker = mywaker_into_waker(Arc::into_raw(mywaker));
  // (Waker, TaskState)
  let mut cx = Context::from_waker(&waker);

  // Prevent the future from being modified by other programs
  // or threads during asynchronous execution.
  let mut future = unsafe { Pin::new_unchecked(&mut future) };
  loop {
    match Future::poll(future.as_mut(), &mut cx) {
      Poll::Ready(val) => break val,
      Poll::Pending => parker.park(),
    }
  }
}

// ==== FUTURE IMPLEMENTATION ====
#[derive(Clone)]
/// Custom wake up
struct MyWaker {
  parker: Arc<Parker>,
}

#[derive(Clone)]
/// An abstraction that encapsulates asynchronous tasks.
pub struct Task {
  /// Status and information used to identify asynchronous tasks.
  id: usize,
  /// A thread pool reference type used to hold the thread pool assigned by the task
  /// and ensure that it can be shared by multiple asynchronous tasks.
  reactor: Arc<Mutex<Box<Reactor>>>,
  /// Used to save data information in asynchronous tasks.
  data: u64,
}

/// virtual method to wake up the thread
fn mywaker_wake(s: &MyWaker) {
  let waker_arc = unsafe { Arc::from_raw(s) };
  waker_arc.parker.unpark();
}

/// virtual method to clone a Waker(RawWaker)
fn mywaker_clone(s: &MyWaker) -> RawWaker {
  let arc = unsafe { Arc::from_raw(s) };
  // Avoid freeing original memory
  std::mem::forget(arc.clone());
  RawWaker::new(Arc::into_raw(arc) as *const (), &VTABLE)
}

/// `const`: fixed in address speedup the run effectively.
const VTABLE: RawWakerVTable = unsafe {
  RawWakerVTable::new(
    |s| mywaker_clone(&*(s as *const MyWaker)),
    // ownership
    |s| mywaker_wake(&*(s as *const MyWaker)),
    // shared borrow
    |s| mywaker_wake(*(s as *const &MyWaker)),
    |s| drop(Arc::from_raw(&*(s as *const MyWaker))),
  )
};

/// combine data `MyWaker` and virtual table `VTABLE` into `RawWaker`
fn mywaker_into_waker(s: *const MyWaker) -> Waker {
  let raw_waker = RawWaker::new(s as *const (), &VTABLE);
  unsafe { Waker::from_raw(raw_waker) }
}

impl Task {
  fn new(reactor: Arc<Mutex<Box<Reactor>>>, data: u64, id: usize) -> Self {
    Task { id, reactor, data }
  }
}

impl Future for Task {
  type Output = usize;

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    let mut r = self.reactor.lock().unwrap();
    if r.is_ready(self.id) {
      *r.tasks.get_mut(&self.id).unwrap() = TaskState::Finished;
      Poll::Ready(self.id)
    } else if let hash_map::Entry::Occupied(mut e) = r.tasks.entry(self.id) {
      e.insert(TaskState::NotReady(cx.waker().clone()));
      Poll::Pending
    } else {
      r.register(self.data, cx.waker().clone(), self.id);
      Poll::Pending
    }
  }
}

// ==== REACTOR ====
enum TaskState {
  Ready,
  NotReady(Waker),
  Finished,
}

struct Reactor {
  /// An asynchronous task event distributor that delivers wake-up and
  ///  event notification information to asynchronous tasks.
  dispatcher: Sender<Event>,
  /// A thread handle object used to manage threads assigned by asynchronous tasks.
  handler: Option<JoinHandle<()>>,
  /// Used to maintain state information and identifiers for asynchronous tasks.
  tasks: HashMap<usize, TaskState>,
}

#[derive(Debug)]
enum Event {
  Close,
  Timeout(u64, usize),
}

impl Reactor {
  fn new() -> Arc<Mutex<Box<Self>>> {
    let (tx, rx) = channel::<Event>();
    let reactor = Arc::new(Mutex::new(Box::new(Reactor {
      dispatcher: tx,
      handler: None,
      tasks: HashMap::new(),
    })));

    let reactor_clone = Arc::downgrade(&reactor);
    // spawn a new `handler thread` to handle asynchronous tasks.
    // capture variable: (reactor_clone)
    let handle = thread::spawn(move || {
      let mut handles = vec![];
      for event in rx {
        let reactor = reactor_clone.clone();
        match event {
          Event::Close => break,
          Event::Timeout(duration, id) => {
            // spawn a new thread to do sleep operation.
            // capture variable: (duration, id)
            let event_handle = thread::spawn(move || {
              thread::sleep(Duration::from_secs(duration));
              // upgrade weak reference to string reference, the reactor must exist.
              let reactor = reactor.upgrade().unwrap();
              // wake the corresponding task by its id.
              reactor.lock().map(|mut r| r.wake(id)).unwrap();
            });
            handles.push(event_handle);
          }
        }
      }
      // wait all handling threads finished.
      handles
        .into_iter()
        .for_each(|handler| handler.join().unwrap());
    });
    // set the handler thread
    reactor
      .lock()
      .map(|mut r| r.handler = Some(handle))
      .unwrap();
    reactor
  }

  /// wake up the task by task_id
  fn wake(&mut self, id: usize) {
    let state = self.tasks.get_mut(&id).unwrap();
    match mem::replace(state, TaskState::Ready) {
      // wake up through the `Waker`
      TaskState::NotReady(waker) => waker.wake(),
      TaskState::Finished => {
        // It means that the task has completed or is being woken up and
        // cannot be woken up again, throwing a panic exception
        panic!("Called `wake` twice on task: {}", id)
      }
      _ => unreachable!(),
    }
  }

  /// register the timeout task,
  /// and insert this task as (TaskState::NotReady(waker)) with waker ownership.
  fn register(&mut self, duration: u64, waker: Waker, id: usize) {
    if self.tasks.insert(id, TaskState::NotReady(waker)).is_some() {
      panic!("Tried to insert a task with id: `{}`, twice!", id);
    }
    self.dispatcher.send(Event::Timeout(duration, id)).unwrap();
  }

  /// Close the reactor
  fn close(&mut self) {
    self.dispatcher.send(Event::Close).unwrap();
  }

  /// Check the task of id, whether it is int TaskState::Ready.
  fn is_ready(&self, id: usize) -> bool {
    self
      .tasks
      .get(&id)
      .map(|state| matches!(state, TaskState::Ready))
      .unwrap_or(false)
  }
}

impl Drop for Reactor {
  fn drop(&mut self) {
    // wait until the handler thread exit.
    self.handler.take().map(|h| h.join().unwrap()).unwrap();
  }
}
