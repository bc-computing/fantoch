use fantoch::id::ProcessId;
use std::collections::hash_map::{Entry, HashMap};
use std::collections::HashSet;

type Ballot = u64;
type Slot = u64;

// The first component is the ballot in which the value (the second component)
// was accepted.
type Accepted<V> = (Ballot, V);
type AcceptedSlots<V> = HashMap<Slot, Accepted<V>>;
type Accepts = HashSet<ProcessId>;

/// Implementation of Flexible multi-decree Paxos in which:
/// - phase-1 waits for n - f promises
/// - phase-2 waits for f + 1 accepts
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultiSynodMessage<V> {
    // to be handled outside of this module
    MChosen(Slot, V),
    MForwardSubmit(V),
    // messages to root mod
    MSpawnCommander(Ballot, Slot, V),
    // messages to acceptor
    MPrepare(Ballot),
    MAccept(Ballot, Slot, V),
    // messages to leader
    MPromise(Ballot, AcceptedSlots<V>),
    // messages to the commander
    MAccepted(Ballot, Slot),
}

#[derive(Clone)]
pub struct MultiSynod<V> {
    // number of processes
    n: usize,
    // maximum number of allowed failures
    f: usize,
    // paxos agents
    leader: Leader,
    acceptor: Acceptor<V>,
    commanders: HashMap<Slot, Commander<V>>,
}

impl<V> MultiSynod<V>
where
    V: Clone,
{
    /// Creates a new `MultiSynod` instance.
    pub fn new(
        process_id: ProcessId,
        initial_leader: ProcessId,
        n: usize,
        f: usize,
    ) -> Self {
        Self {
            n,
            f,
            leader: Leader::new(process_id, initial_leader),
            acceptor: Acceptor::new(initial_leader),
            commanders: HashMap::new(),
        }
    }

    pub fn submit(&mut self, value: V) -> MultiSynodMessage<V> {
        if let Some((ballot, slot)) = self.leader.try_submit() {
            // if we're the leader, create a spawn commander message:
            // - this message is to be handled locally, but it can be handled in
            //   a different local multi-synod process for parallelism
            MultiSynodMessage::MSpawnCommander(ballot, slot, value)
        } else {
            // if we're not the leader, then create an `MForwardSubmit` to be
            // sent to the leader
            MultiSynodMessage::MForwardSubmit(value)
        }
    }

    /// Handles `MultiSynodMessage`s generated by this `MultiSynod` module by
    /// forwarding them to the proper agent.
    pub fn handle(
        &mut self,
        from: ProcessId,
        msg: MultiSynodMessage<V>,
    ) -> Option<MultiSynodMessage<V>> {
        match msg {
            // handle spawn commander
            MultiSynodMessage::MSpawnCommander(b, slot, value) => {
                let maccept = self.handle_spawn_commander(b, slot, value);
                Some(maccept)
            }
            // handle messages to acceptor
            MultiSynodMessage::MPrepare(b) => self.acceptor.handle_prepare(b),
            MultiSynodMessage::MAccept(b, slot, value) => {
                self.acceptor.handle_accept(b, slot, value)
            }
            // handle messages to leader
            MultiSynodMessage::MPromise(_b, _previous) => {
                todo!("handling of MultiSynodMessage::MPromise not implemented yet");
            }
            // handle messages to comamnders
            MultiSynodMessage::MAccepted(b, slot) => {
                self.handle_maccepted(from, b, slot)
            }
            MultiSynodMessage::MChosen(_, _) => panic!("MultiSynod::MChosen messages are to be handled outside of MultiSynod"),
            MultiSynodMessage::MForwardSubmit(_) => panic!("MultiSynod::MForwardSubmit messages are to be handled outside of MultiSynod")
        }
    }

    /// Performs garbage collection of stable slots.
    pub fn gc(&mut self, stable: Vec<u64>) {
        self.acceptor.gc(stable);
    }

    fn handle_spawn_commander(
        &mut self,
        ballot: Ballot,
        slot: Slot,
        value: V,
    ) -> MultiSynodMessage<V> {
        // create a new commander
        let commander = Commander::spawn(self.f, ballot, value.clone());
        // update list of commander
        let res = self.commanders.insert(slot, commander);
        // check that there was no other commander for this slot
        assert!(res.is_none());
        // create the accept message
        MultiSynodMessage::MAccept(ballot, slot, value)
    }

    fn handle_maccepted(
        &mut self,
        from: ProcessId,
        ballot: Ballot,
        slot: Slot,
    ) -> Option<MultiSynodMessage<V>> {
        // get the commander of this slot:
        match self.commanders.entry(slot) {
            Entry::Occupied(mut entry) => {
                let commander = entry.get_mut();
                let chosen = commander.handle_accepted(from, ballot);
                // if the commander has gathered enough accepts, then
                // the value for this slot is chosen
                if chosen {
                    // destroy commander and get the value that was
                    // being watched
                    let value = entry.remove().destroy();
                    // create chosen message
                    let chosen = MultiSynodMessage::MChosen(slot, value);
                    Some(chosen)
                } else {
                    None
                }
            }
            Entry::Vacant(_) => {
                // ignore message if commander does not exist
                println!("MultiSynodMesssage::MAccepted({}, {}) ignored as a commander for that slot {} does not exist", ballot, slot, slot);
                None
            }
        }
    }
}

#[derive(Clone)]
struct Leader {
    // process identifier
    process_id: ProcessId,
    // flag indicating whether we're the leader
    is_leader: bool,
    // ballot to be used in accept messages
    ballot: Ballot,
    // last slot used in accept messages
    last_slot: Slot,
}

impl Leader {
    /// Creates a new leader.
    fn new(process_id: ProcessId, initial_leader: ProcessId) -> Self {
        // we're leader if the identifier of the initial leader is us
        let is_leader = process_id == initial_leader;
        // if we're the leader, then use as initial ballot our id, which will
        // automatically be joined by all acceptors on bootstrap
        let ballot = if is_leader { process_id } else { 0 };
        // last slot is 0
        let last_slot = 0;
        Self {
            process_id,
            is_leader,
            ballot,
            last_slot,
        }
    }

    /// Tries to submit a command. If we're the leader, then the leader ballot
    /// and a new slot will be returned.
    fn try_submit(&mut self) -> Option<(Ballot, Slot)> {
        if self.is_leader {
            // increase slot
            self.last_slot += 1;
            // return ballot and slot
            Some((self.ballot, self.last_slot))
        } else {
            None
        }
    }
}

#[derive(Clone)]
struct Commander<V> {
    // maximum number of allowed failures
    f: usize,
    // ballot prepared by the leader
    ballot: Ballot,
    // value sent in the accept
    value: V,
    // set of processes that have accepted the accept
    accepts: Accepts,
}

impl<V> Commander<V>
where
    V: Clone,
{
    // Spawns a new commander to watch accepts on some slot.
    fn spawn(f: usize, ballot: Ballot, value: V) -> Self {
        Self {
            f,
            ballot,
            value,
            accepts: HashSet::new(),
        }
    }

    // Processes an accepted message, returning a bool indicating whether we
    // have enough accepts.
    fn handle_accepted(&mut self, from: ProcessId, b: Ballot) -> bool {
        // check if it's an accept about the current ballot (so that we only
        // process accepts about the current ballot)
        if self.ballot == b {
            // if yes, update set of accepts
            self.accepts.insert(from);

            // check if we have enough (i.e. f + 1) accepts
            self.accepts.len() == self.f + 1
        } else {
            false
        }
    }

    // Destroys the commander, returning the value being watched. This should be
    // called once `handle_accepted` returns true. It will panic otherwise.
    fn destroy(self) -> V {
        assert_eq!(self.accepts.len(), self.f + 1);
        self.value
    }
}

#[derive(Clone)]
struct Acceptor<Value> {
    ballot: Ballot,
    accepted: HashMap<Slot, Accepted<Value>>,
}

impl<V> Acceptor<V>
where
    V: Clone,
{
    // Creates a new acceptor given the initial leader.
    // The acceptor immediately joins the first ballot of this leader, i.e. its
    // identifer.
    fn new(initial_leader: ProcessId) -> Self {
        Self {
            ballot: initial_leader,
            accepted: HashMap::new(),
        }
    }

    // The reply to this prepare request contains:
    // - a promise to never accept a proposal numbered less than `b`
    // - the non-GCed proposals accepted at ballots less than `b`, if any
    fn handle_prepare(&mut self, b: Ballot) -> Option<MultiSynodMessage<V>> {
        // since we need to promise that we won't accept any proposal numbered
        // less then `b`, there's no point in letting such proposal be
        // prepared, and so, we ignore such prepares
        if b > self.ballot {
            // update current ballot
            self.ballot = b;
            // create promise message
            let promise = MultiSynodMessage::MPromise(b, self.accepted.clone());
            Some(promise)
        } else {
            None
        }
    }

    fn handle_accept(
        &mut self,
        b: Ballot,
        slot: Slot,
        value: V,
    ) -> Option<MultiSynodMessage<V>> {
        if b >= self.ballot {
            // update current ballot
            self.ballot = b;
            // update the accepted value for `slot`
            self.accepted.insert(slot, (b, value));
            // create accepted message
            let accepted = MultiSynodMessage::MAccepted(b, slot);
            Some(accepted)
        } else {
            None
        }
    }

    /// Performs garbage collection of stable slots.
    fn gc(&mut self, stable: Vec<u64>) {
        stable.iter().for_each(|slot| {
            self.accepted.remove(&slot);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_synod_flow() {
        // n and f
        let n = 3;
        let f = 1;

        // initial leader is 1
        let initial_leader = 1;

        // create all synods
        let mut synod_1 = MultiSynod::<usize>::new(1, initial_leader, n, f);
        let mut synod_2 = MultiSynod::<usize>::new(2, initial_leader, n, f);
        let mut synod_3 = MultiSynod::<usize>::new(3, initial_leader, n, f);

        // synod 1: submit new command
        let value = 10;
        let spawn = synod_1.submit(value);
        // since synod 1 is the leader, then the message is a spawn commander
        match &spawn {
            MultiSynodMessage::MSpawnCommander(_, _, _) => {}
            _ => panic!(
                "submitting at the leader should create an spawn commander message"
            ),
        };

        let accept =
            synod_1.handle(1, spawn).expect("there should be an accept");
        // handle the spawn commander locally creating an accept message
        match &accept {
            MultiSynodMessage::MAccept(_, _, _) => {}
            _ => panic!(
                "the handle of a spawn commander should result in an accept message"
            ),
        };

        // handle the accept at f + 1 processes, including synod 1
        let accepted_1 = synod_1
            .handle(1, accept.clone())
            .expect("there should an accept from 1");
        let accepted_2 = synod_2
            .handle(1, accept.clone())
            .expect("there should an accept from 2");

        // synod 1: handle accepts
        let result = synod_1.handle(1, accepted_1);
        assert!(result.is_none());
        let chosen = synod_1
            .handle(2, accepted_2)
            .expect("there should be a chosen message");

        // check that `valeu` was chosen at slot 1
        let slot = 1;
        assert_eq!(chosen, MultiSynodMessage::MChosen(slot, value));

        // synod 3: submit new command
        // since synod 3 is *not* the leader, then the message is an mforward
        let value = 30;
        match synod_3.submit(value) {
            MultiSynodMessage::MForwardSubmit(forward_value) => {
                assert_eq!(value, forward_value)
            }
            _ => panic!(
                "submitting at a non-leader should create an mfoward message"
            ),
        };
    }
}
