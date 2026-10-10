//! A deterministic model for actor request/message admission.
//!
//! NOT the production Oreslang runtime. This models the target scheduler
//! invariants for requestConcurrency/messageConcurrency > 1 without ever
//! allowing two simultaneous actor execution turns.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub requests: usize,
    pub messages_per_request: usize,
}

impl Limits {
    pub fn new(requests: usize, messages_per_request: usize) -> Result<Self, Error> {
        if requests == 0 || messages_per_request == 0
            || requests > 1024 || messages_per_request > 1024
            || requests.saturating_mul(messages_per_request) > 4096
        {
            return Err(Error::InvalidLimit);
        }
        Ok(Self { requests, messages_per_request })
    }

    pub fn serial() -> Self { Self { requests: 1, messages_per_request: 1 } }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidLimit,
    RequestLimit,
    MessageLimit,
    DuplicateRequest,
    DuplicateMessage,
    UnknownRequest,
    UnknownMessage,
    RequestNotEmpty,
    TurnBusy,
    NotRunnable,
    WrongLease,
    TurnStillActive,
    StaleRequest,
    StaleMessage,
    EpochExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase { Ready, Running, Suspended }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lease {
    request: u64,
    message: u64,
    token: u64,
}

/// Request/message handles are lifetime-specific scheduler capabilities. IDs
/// supplied by callers are only routing keys and may be reused after closing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestHandle { id: u64, incarnation: u64 }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MessageHandle {
    request: RequestHandle,
    id: u64,
    incarnation: u64,
}

#[derive(Default)]
struct Request {
    incarnation: u64,
    messages: BTreeMap<u64, Phase>,
    message_incarnations: BTreeMap<u64, u64>,
}

pub struct Admission {
    limits: Limits,
    requests: BTreeMap<u64, Request>,
    active: Option<Lease>,
    next_token: u64,
    next_request_incarnation: u64,
    next_message_incarnation: u64,
}

impl Admission {
    pub fn new(limits: Limits) -> Self {
        Self { limits, requests: BTreeMap::new(), active: None, next_token: 0,
            next_request_incarnation: 0, next_message_incarnation: 0 }
    }

    pub fn active_lease(&self) -> Option<Lease> { self.active }
    pub fn request_count(&self) -> usize { self.requests.len() }

    // ID-only methods are private model test helpers. Runtime callers must use
    // the public lifetime-capability API below to fence delayed IO callbacks.
    fn admit_request(&mut self, id: u64) -> Result<(), Error> {
        if self.requests.contains_key(&id) { return Err(Error::DuplicateRequest); }
        if self.requests.len() >= self.limits.requests { return Err(Error::RequestLimit); }
        let incarnation = self.next_request_incarnation.checked_add(1)
            .ok_or(Error::EpochExhausted)?;
        self.next_request_incarnation = incarnation;
        self.requests.insert(id, Request {
            incarnation, ..Request::default()
        });
        Ok(())
    }

    fn admit_message(&mut self, request: u64, message: u64) -> Result<(), Error> {
        let r = self.requests.get_mut(&request).ok_or(Error::UnknownRequest)?;
        if r.messages.contains_key(&message) { return Err(Error::DuplicateMessage); }
        if r.messages.len() >= self.limits.messages_per_request {
            return Err(Error::MessageLimit);
        }
        let incarnation = self.next_message_incarnation.checked_add(1)
            .ok_or(Error::EpochExhausted)?;
        self.next_message_incarnation = incarnation;
        r.messages.insert(message, Phase::Ready);
        r.message_incarnations.insert(message, incarnation);
        Ok(())
    }

    fn check_request(&self, handle: RequestHandle) -> Result<(), Error> {
        match self.requests.get(&handle.id) {
            Some(r) if r.incarnation == handle.incarnation => Ok(()),
            Some(_) => Err(Error::StaleRequest),
            None => Err(Error::UnknownRequest),
        }
    }

    fn check_message(&self, handle: MessageHandle) -> Result<(), Error> {
        self.check_request(handle.request)?;
        match self.requests.get(&handle.request.id)
            .and_then(|r| r.message_incarnations.get(&handle.id)) {
            Some(i) if *i == handle.incarnation => Ok(()),
            Some(_) => Err(Error::StaleMessage),
            None => Err(Error::UnknownMessage),
        }
    }

    /// Admit an externally correlated request lifetime, returning an opaque
    /// incarnation handle. Never derive a fresh handle in a late callback.
    pub fn open_request(&mut self, id: u64) -> Result<RequestHandle, Error> {
        self.admit_request(id)?;
        let incarnation = self.requests.get(&id).ok_or(Error::UnknownRequest)?.incarnation;
        Ok(RequestHandle { id, incarnation })
    }

    pub fn open_message(&mut self, request: RequestHandle, id: u64)
        -> Result<MessageHandle, Error> {
        self.check_request(request)?;
        self.admit_message(request.id, id)?;
        let incarnation = *self.requests.get(&request.id)
            .and_then(|r| r.message_incarnations.get(&id))
            .ok_or(Error::UnknownMessage)?;
        Ok(MessageHandle { request, id, incarnation })
    }

    pub fn enter_message(&mut self, message: MessageHandle) -> Result<Lease, Error> {
        self.check_message(message)?;
        self.enter(message.request.id, message.id)
    }

    pub fn wake_message(&mut self, message: MessageHandle) -> Result<(), Error> {
        self.check_message(message)?;
        self.wake(message.request.id, message.id)
    }

    pub fn finish_message_handle(&mut self, message: MessageHandle) -> Result<(), Error> {
        self.check_message(message)?;
        self.finish_message(message.request.id, message.id)
    }

    pub fn cancel_request_handle(&mut self, request: RequestHandle) -> Result<(), Error> {
        self.check_request(request)?;
        self.cancel_request(request.id)
    }

    pub fn finish_request_handle(&mut self, request: RequestHandle) -> Result<(), Error> {
        self.check_request(request)?;
        self.finish_request(request.id)
    }

    fn enter(&mut self, request: u64, message: u64) -> Result<Lease, Error> {
        if self.active.is_some() { return Err(Error::TurnBusy); }
        let phase = self.requests.get_mut(&request)
            .ok_or(Error::UnknownRequest)?.messages.get_mut(&message)
            .ok_or(Error::UnknownMessage)?;
        if *phase != Phase::Ready { return Err(Error::NotRunnable); }
        let token = self.next_token.checked_add(1).ok_or(Error::InvalidLimit)?;
        self.next_token = token;
        let lease = Lease { request, message, token };
        *phase = Phase::Running;
        self.active = Some(lease);
        Ok(lease)
    }

    /// Suspension ends only this *execution turn*. Its message lifetime
    /// remains admitted, and the scheduler can admit a different ready turn.
    pub fn suspend(&mut self, lease: Lease) -> Result<(), Error> {
        self.transition(lease, Phase::Suspended)
    }

    /// A turn may complete without ending its containing message lifetime,
    /// e.g. a cooperative non-suspending yield.
    pub fn yield_turn(&mut self, lease: Lease) -> Result<(), Error> {
        self.transition(lease, Phase::Ready)
    }

    fn transition(&mut self, lease: Lease, to: Phase) -> Result<(), Error> {
        if self.active != Some(lease) { return Err(Error::WrongLease); }
        let current = self.requests.get_mut(&lease.request)
            .ok_or(Error::UnknownRequest)?.messages.get_mut(&lease.message)
            .ok_or(Error::UnknownMessage)?;
        if *current != Phase::Running { return Err(Error::NotRunnable); }
        *current = to;
        self.active = None;
        Ok(())
    }

    fn wake(&mut self, request: u64, message: u64) -> Result<(), Error> {
        let phase = self.requests.get_mut(&request)
            .ok_or(Error::UnknownRequest)?.messages.get_mut(&message)
            .ok_or(Error::UnknownMessage)?;
        if *phase != Phase::Suspended { return Err(Error::NotRunnable); }
        *phase = Phase::Ready;
        Ok(())
    }

    fn finish_message(&mut self, request: u64, message: u64) -> Result<(), Error> {
        if self.active.is_some_and(|lease|
            lease.request == request && lease.message == message) {
            return Err(Error::TurnStillActive);
        }
        let r = self.requests.get_mut(&request).ok_or(Error::UnknownRequest)?;
        if r.messages.remove(&message).is_none() { return Err(Error::UnknownMessage); }
        r.message_incarnations.remove(&message);
        Ok(())
    }

    /// Cancellation fences queued and suspended continuation identities.
    /// Active guest code cannot be revoked while holding its actor execution
    /// lease: request a safepoint unwind, then call again after lease release.
    /// A late wake from an old IO completion will be UnknownMessage.
    fn cancel_request(&mut self, request: u64) -> Result<(), Error> {
        if self.active.is_some_and(|lease| lease.request == request) {
            return Err(Error::TurnStillActive);
        }
        if self.requests.remove(&request).is_none() {
            return Err(Error::UnknownRequest);
        }
        Ok(())
    }

    fn finish_request(&mut self, request: u64) -> Result<(), Error> {
        let r = self.requests.get(&request).ok_or(Error::UnknownRequest)?;
        if !r.messages.is_empty() { return Err(Error::RequestNotEmpty); }
        self.requests.remove(&request);
        Ok(())
    }

    /// A bounded state invariant, suitable for refinement testing against
    /// the eventual source actor implementation.
    pub fn check_invariants(&self) -> bool {
        if self.requests.len() > self.limits.requests { return false; }
        let mut running = 0;
        for r in self.requests.values() {
            if r.messages.len() > self.limits.messages_per_request { return false; }
            if r.messages.len() != r.message_incarnations.len() { return false; }
            if r.messages.keys().any(|id| !r.message_incarnations.contains_key(id)) {
                return false;
            }
            running += r.messages.values().filter(|x| **x == Phase::Running).count();
        }
        if running > 1 || running != usize::from(self.active.is_some()) { return false; }
        if let Some(lease) = self.active {
            self.requests.get(&lease.request)
                .and_then(|r| r.messages.get(&lease.message))
                == Some(&Phase::Running)
        } else { true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_is_a_logical_critical_section_across_await() {
        let mut a = Admission::new(Limits::serial());
        a.admit_request(1).unwrap();
        a.admit_message(1, 10).unwrap();
        let lease = a.enter(1, 10).unwrap();
        a.suspend(lease).unwrap();
        assert_eq!(a.admit_request(2), Err(Error::RequestLimit));
        assert_eq!(a.admit_message(1, 11), Err(Error::MessageLimit));
        assert_eq!(a.enter(1, 10), Err(Error::NotRunnable));
        a.wake(1, 10).unwrap();
        let resumed = a.enter(1, 10).unwrap();
        assert_ne!(lease, resumed);
        a.yield_turn(resumed).unwrap();
        a.finish_message(1, 10).unwrap();
        a.finish_request(1).unwrap();
        a.admit_request(2).unwrap();
        assert!(a.check_invariants());
    }

    #[test]
    fn three_requests_with_four_messages_each_never_create_parallel_native_turns() {
        let mut a = Admission::new(Limits::new(3, 4).unwrap());
        for r in 0..3 {
            a.admit_request(r).unwrap();
            for m in 0..4 { a.admit_message(r, m).unwrap(); }
        }
        assert_eq!(a.admit_request(3), Err(Error::RequestLimit));
        assert_eq!(a.admit_message(0, 4), Err(Error::MessageLimit));
        let one = a.enter(0, 0).unwrap();
        assert_eq!(a.enter(2, 3), Err(Error::TurnBusy));
        assert!(a.check_invariants());
        a.suspend(one).unwrap();
        let two = a.enter(2, 3).unwrap();
        assert!(a.check_invariants());
        assert_eq!(a.enter(1, 1), Err(Error::TurnBusy));
        a.yield_turn(two).unwrap();
        a.wake(0, 0).unwrap();
        let resumed = a.enter(0, 0).unwrap();
        a.yield_turn(resumed).unwrap();
        assert!(a.check_invariants());
    }

    #[test]
    fn duplicate_requests_messages_and_early_request_close_fail() {
        let mut a = Admission::new(Limits::new(3, 4).unwrap());
        a.admit_request(42).unwrap();
        assert_eq!(a.admit_request(42), Err(Error::DuplicateRequest));
        a.admit_message(42, 1).unwrap();
        assert_eq!(a.admit_message(42, 1), Err(Error::DuplicateMessage));
        assert_eq!(a.finish_request(42), Err(Error::RequestNotEmpty));
        let lease = a.enter(42, 1).unwrap();
        assert_eq!(a.finish_message(42, 1), Err(Error::TurnStillActive));
        a.yield_turn(lease).unwrap();
        a.finish_message(42, 1).unwrap();
        a.finish_request(42).unwrap();
    }

    #[test]
    fn stale_lease_cannot_end_another_message_turn() {
        let mut a = Admission::new(Limits::new(2, 2).unwrap());
        a.admit_request(1).unwrap();
        a.admit_message(1, 1).unwrap();
        a.admit_message(1, 2).unwrap();
        let stale = a.enter(1, 1).unwrap();
        a.yield_turn(stale).unwrap();
        let current = a.enter(1, 2).unwrap();
        assert_eq!(a.suspend(stale), Err(Error::WrongLease));
        assert_eq!(a.active_lease(), Some(current));
        assert!(a.check_invariants());
        a.suspend(current).unwrap();
    }

    #[test]
    fn cancelled_suspended_request_cannot_be_resurrected_by_late_wakeup() {
        let mut a = Admission::new(Limits::new(2, 2).unwrap());
        a.admit_request(1).unwrap();
        a.admit_message(1, 100).unwrap();
        let turn = a.enter(1, 100).unwrap();
        assert_eq!(a.cancel_request(1), Err(Error::TurnStillActive));
        assert_eq!(a.active_lease(), Some(turn));
        a.suspend(turn).unwrap();
        assert_eq!(a.cancel_request(1), Ok(()));
        assert_eq!(a.wake(1, 100), Err(Error::UnknownRequest));
        assert_eq!(a.finish_message(1, 100), Err(Error::UnknownRequest));
        assert!(a.check_invariants());
    }

    #[test]
    fn failed_admission_and_turn_transitions_leave_existing_lease_intact() {
        let mut a = Admission::new(Limits::new(1, 1).unwrap());
        a.admit_request(1).unwrap();
        a.admit_message(1, 1).unwrap();
        let turn = a.enter(1, 1).unwrap();
        assert_eq!(a.admit_request(2), Err(Error::RequestLimit));
        assert_eq!(a.admit_message(1, 2), Err(Error::MessageLimit));
        assert_eq!(a.enter(1, 1), Err(Error::TurnBusy));
        assert_eq!(a.wake(1, 1), Err(Error::NotRunnable));
        assert_eq!(a.finish_message(1, 1), Err(Error::TurnStillActive));
        assert_eq!(a.active_lease(), Some(turn));
        assert!(a.check_invariants());
    }

    #[test]
    fn deterministic_interleaving_exercises_suspension_and_cancellation() {
        let mut a = Admission::new(Limits::new(3, 4).unwrap());
        let mut seed = 0x99e3_c845_1465_3081_u64;
        for iteration in 0..4000u64 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let request = seed % 5;
            let message = (seed >> 8) % 6;
            match seed % 7 {
                0 => { let _ = a.admit_request(request); }
                1 => { let _ = a.admit_message(request, message); }
                2 => { let _ = a.wake(request, message); }
                3 => { let _ = a.finish_message(request, message); }
                4 => { let _ = a.cancel_request(request); }
                5 => {
                    if let Ok(lease) = a.enter(request, message) {
                        if iteration % 2 == 0 {
                            a.suspend(lease).unwrap();
                        } else {
                            a.yield_turn(lease).unwrap();
                        }
                    }
                }
                _ => {
                    if let Some(lease) = a.active_lease() {
                        a.yield_turn(lease).unwrap();
                    }
                }
            }
            assert!(a.check_invariants(), "violation on deterministic event {iteration}");
        }
    }

    #[test]
    fn invalid_limits_are_rejected() {
        assert_eq!(Limits::new(0, 1), Err(Error::InvalidLimit));
        assert_eq!(Limits::new(3, 0), Err(Error::InvalidLimit));
        assert_eq!(Limits::new(1025, 1), Err(Error::InvalidLimit));
        assert_eq!(Limits::new(100, 100), Err(Error::InvalidLimit));
        assert_eq!(Limits::new(3, 4), Ok(Limits { requests: 3, messages_per_request: 4 }));
    }
    #[test]
    fn stale_callbacks_cannot_resurrect_reused_request_and_message_ids() {
        let mut a = Admission::new(Limits::new(2, 2).unwrap());
        let first_request = a.open_request(9).unwrap();
        let first_message = a.open_message(first_request, 5).unwrap();
        let turn = a.enter_message(first_message).unwrap();
        a.suspend(turn).unwrap();
        a.cancel_request_handle(first_request).unwrap();

        let second_request = a.open_request(9).unwrap();
        let second_message = a.open_message(second_request, 5).unwrap();
        let second_turn = a.enter_message(second_message).unwrap();
        a.suspend(second_turn).unwrap();

        assert_ne!(first_request, second_request);
        assert_ne!(first_message, second_message);
        assert_eq!(a.wake_message(first_message), Err(Error::StaleRequest));
        assert_eq!(a.finish_message_handle(first_message), Err(Error::StaleRequest));
        assert_eq!(a.cancel_request_handle(first_request), Err(Error::StaleRequest));
        assert_eq!(a.finish_request_handle(first_request), Err(Error::StaleRequest));
        assert_eq!(a.wake_message(second_message), Ok(()));
        assert!(a.check_invariants());
    }

    #[test]
    fn stale_message_incarnation_cannot_cancel_replacement_in_same_request() {
        let mut a = Admission::new(Limits::serial());
        let request = a.open_request(5).unwrap();
        let previous = a.open_message(request, 8).unwrap();
        a.finish_message_handle(previous).unwrap();
        let current = a.open_message(request, 8).unwrap();
        assert_ne!(previous, current);
        assert_eq!(a.wake_message(previous), Err(Error::StaleMessage));
        assert_eq!(a.finish_message_handle(previous), Err(Error::StaleMessage));
        let turn = a.enter_message(current).unwrap();
        a.yield_turn(turn).unwrap();
        a.finish_message_handle(current).unwrap();
        a.finish_request_handle(request).unwrap();
        assert!(a.check_invariants());
    }

}
