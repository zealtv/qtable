//! A sleeping monotonic wake thread bridges the hub's condvar to iced. It emits no idle
//! messages, and holds at most one notification while the GUI owner is busy.
use super::{Action, State};
use crate::messages::Message;
use iced::futures::{SinkExt, future};
use iced::{Subscription, stream};
use qtable_remote::Ingress;
use std::hash::{Hash, Hasher};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone)]
struct Source(Arc<Ingress>);
impl Hash for Source {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

struct Stop {
    ingress: Arc<Ingress>,
    stop: Arc<AtomicBool>,
}
impl Drop for Stop {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.ingress.reset_wake();
        self.ingress.notify_host();
    }
}

pub(crate) fn subscription(state: &State) -> Subscription<Message> {
    if state.runtime.is_none() {
        return Subscription::none();
    }
    Subscription::run_with(Source(state.ingress.clone()), |source| {
        let ingress = source.0.clone();
        stream::channel(1, async move |mut output| {
            let stop = Arc::new(AtomicBool::new(false));
            let guard = Stop {
                ingress: ingress.clone(),
                stop: stop.clone(),
            };
            let mut worker_output = output.clone();
            let worker = std::thread::Builder::new()
                .name("qtable-midi-wake".into())
                .spawn(move || {
                    while ingress.wait_for_wake(&stop) {
                        if iced::futures::executor::block_on(
                            worker_output.send(Message::Remote(Action::Wake)),
                        )
                        .is_err()
                        {
                            break;
                        }
                    }
                });
            if let Err(error) = worker {
                let _ = output
                    .send(Message::Remote(Action::Error(format!(
                        "Could not start MIDI wake: {error}"
                    ))))
                    .await;
            }
            let _guard = guard;
            future::pending::<()>().await;
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopping_subscription_releases_its_outstanding_wake() {
        let ingress = Arc::new(Ingress::default());
        let stop = Arc::new(AtomicBool::new(false));
        ingress.notify_host();
        assert!(ingress.wait_for_wake(&stop));
        drop(Stop {
            ingress: ingress.clone(),
            stop: stop.clone(),
        });
        assert!(stop.load(Ordering::Acquire));
        // Use a bounded wait so a regression reports failure instead of hanging.
        let (tx, rx) = std::sync::mpsc::channel();
        let next_stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let ingress = ingress.clone();
            let next_stop = next_stop.clone();
            std::thread::spawn(move || tx.send(ingress.wait_for_wake(&next_stop)).unwrap())
        };
        let wake = rx.recv_timeout(std::time::Duration::from_secs(2));
        next_stop.store(true, Ordering::Release);
        ingress.notify_host();
        worker.join().unwrap();
        assert!(wake.unwrap());
    }
}
