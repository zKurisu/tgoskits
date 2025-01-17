use alloc::{boxed::Box, vec::Vec};
use core::{cmp::Eq, fmt::Debug};
use log::debug;

#[derive(Debug)]
pub struct Queue {
    events: Vec<Event>,
}

pub struct Event {
    pub at_tick: u64,
    pub interval: Option<u64>,
    pub called: bool,
    pub callback: Box<dyn Fn()>,
}

impl Debug for Event {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Event")
            .field("at_tick", &self.at_tick)
            .field("interval", &self.interval)
            .field("called", &self.called)
            .finish()
    }
}

impl Queue {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn add_and_next_tick(&mut self, event: Event) -> u64 {
        self.events.retain(|e| !e.called);
        self.events.push(event);
        self.events.sort_by(|a, b| a.at_tick.cmp(&b.at_tick));
        self.events[0].at_tick
    }

    pub fn next_tick(&self) -> Option<u64> {
        for e in self.events.iter() {
            if e.called {
                continue;
            }

            return Some(e.at_tick);
        }
        None
    }

    pub fn pop(&mut self, now: u64) -> Option<&Event> {
        for e in self.events.iter_mut() {
            if e.called {
                continue;
            }
            if e.at_tick <= now {
                if let Some(interval) = e.interval {
                    e.at_tick += interval;
                } else {
                    e.called = true;
                }
                return Some(e);
            }
        }
        None
    }
}
