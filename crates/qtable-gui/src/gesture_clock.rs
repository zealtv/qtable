//! Monotonic time for pointer gestures. Production uses the system clock; widget tests
//! can hold time still across view rebuilds and advance it explicitly between events.
use std::time::Instant;

pub(crate) fn now() -> Instant {
    #[cfg(test)]
    if let Some(now) = test::NOW.with(std::cell::Cell::get) {
        return now;
    }
    Instant::now()
}

#[cfg(test)]
pub(crate) mod test {
    use std::cell::Cell;
    use std::marker::PhantomData;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    thread_local! {
        pub(super) static NOW: Cell<Option<Instant>> = const { Cell::new(None) };
    }

    /// Scoped to this test thread, restored on drop (including panic unwinding).
    /// Keep the guard alive while dispatching events; advance before the next press.
    pub(crate) struct Clock {
        previous: Option<Instant>,
        _thread: PhantomData<Rc<()>>,
    }

    impl Clock {
        pub(crate) fn new() -> Self {
            Self {
                previous: NOW.with(|now| now.replace(Some(super::now()))),
                _thread: PhantomData,
            }
        }

        pub(crate) fn advance(&self, elapsed: Duration) {
            NOW.with(|now| now.set(Some(now.get().unwrap() + elapsed)));
        }
    }

    impl Drop for Clock {
        fn drop(&mut self) {
            NOW.with(|now| now.set(self.previous));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{now, test::Clock};
    use std::time::Duration;

    #[test]
    fn scoped_time_is_explicit_nested_and_isolated_from_other_test_threads() {
        assert!(super::test::NOW.with(|time| time.get().is_none()));
        {
            let clock = Clock::new();
            let start = now();
            assert_eq!(now(), start);
            clock.advance(Duration::from_millis(100));
            assert_eq!(now(), start + Duration::from_millis(100));
            {
                let nested = Clock::new();
                nested.advance(Duration::from_secs(1));
                assert_eq!(now(), start + Duration::from_millis(1100));
            }
            assert_eq!(now(), start + Duration::from_millis(100));
            std::thread::spawn(|| {
                assert!(super::test::NOW.with(|time| time.get().is_none()));
            })
            .join()
            .unwrap();
        }
        assert!(super::test::NOW.with(|time| time.get().is_none()));
    }
}
