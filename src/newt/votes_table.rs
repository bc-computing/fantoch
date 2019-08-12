use crate::base::ProcId;
use crate::command::{Command, Key, MultiCommand};
use crate::newt::votes::{VoteRange, Votes};
use std::collections::{BTreeMap, HashMap};
use threshold::AEClock;

pub struct MultiVotesTable {
    stability_threshold: usize,
    tables: HashMap<Key, VotesTable>,
}

impl MultiVotesTable {
    /// Create a new `MultiVotesTable` instance given the stability threshold.
    pub fn new(stability_threshold: usize) -> Self {
        MultiVotesTable {
            stability_threshold,
            tables: HashMap::new(),
        }
    }

    /// Add a new command, its clock and votes to the votes table.
    pub fn add(
        &mut self,
        proc_id: ProcId,
        cmd: Option<MultiCommand>,
        clock: u64,
        votes: Votes,
    ) -> Vec<Command> {
        // do nothing if noOp
        if cmd.is_none() {
            return vec![];
        }
        // else, get an iterator of the actual command
        let mut cmd = cmd.unwrap().into_iter();

        // create sort identifier:
        // - if two commands got assigned the same clock, they will be ordered
        //   by the process id
        let sort_id = (clock, proc_id);

        // add commands and votes to the votes tables, and at the same time
        // compute which commands are safe to be executed
        let to_execute = votes
            .into_iter()
            .flat_map(|(key, vote_ranges)| {
                // the next in cmd must be about the same key
                let (cmd_key, cmd_action) = cmd.next().unwrap();
                assert_eq!(key, cmd_key);

                // TODO can we avoid the next statement? if we do e.g. a
                // `or_insert_with`, the borrow checker will complain
                let empty_table = VotesTable::new(self.stability_threshold);

                // get this key's table
                let table = self.tables.entry(key).or_insert(empty_table);

                // add command and votes to the table
                table.add(sort_id, cmd_action, vote_ranges);

                // get new commands to be executed
                table.stable_commands()
            })
            .collect();

        // check there's nothing else in the cmd iterator
        assert!(cmd.next().is_none());

        // return commands to be executed
        to_execute
    }
}

type SortId = (u64, ProcId);

struct VotesTable {
    stability_threshold: usize,
    votes: AEClock<ProcId>,
    cmds: BTreeMap<SortId, Command>,
}

impl VotesTable {
    fn new(stability_threshold: usize) -> Self {
        VotesTable {
            stability_threshold,
            votes: AEClock::new(),
            cmds: BTreeMap::new(),
        }
    }

    fn add(
        &mut self,
        sort_id: SortId,
        cmd_action: Command,
        vote_ranges: Vec<VoteRange>,
    ) {
        // add command to the sorted list of commands to be executed
        let res = self.cmds.insert(sort_id, cmd_action);
        // and check there was nothing there for this exact same position
        assert!(res.is_none());

        // update votes with the votes used on this command
        // TODO the following step could be more efficient if `threshold::Clock`
        // supports adding ranges to the clock add all vote ranges to votes
        vote_ranges.into_iter().for_each(|vote_range| {
            vote_range.votes().into_iter().for_each(|vote| {
                self.votes.add(&vote_range.voter(), vote);
            })
        });
    }

    fn stable_commands(&mut self) -> impl Iterator<Item = Command> {
        // compute the (potentially) new stable clock for this key
        let stable_clock = self
            .votes
            .frontier_threshold(self.stability_threshold)
            .unwrap();

        // compute stable sort id:
        // - if clock 10 is stable, then we can execute all commands with an id
        //   smaller than `(11,0)`
        // - if id with `(11,0)` is also part of this local structure, we can
        //   also execute it without 11 being stable, because, once 11 is
        //   stable, it will be the first to be executed either way
        let stable_sort_id = (stable_clock + 1, 0);

        // in fact, in the above example, if `(11,0)` is executed, we can also
        // execute `(11,1)`, and with that, execute `(11,2)` and so on
        // TODO loop while the previous flow is true and also return those
        // commands

        // compute the list of commands that can be executed now
        let stable = {
            let mut unstable = self.cmds.split_off(&stable_sort_id);
            // swap unstable with self.cmds
            std::mem::swap(&mut unstable, &mut self.cmds);
            // now unstable contains in fact the stable
            unstable
        };

        // return stable commands
        stable.into_iter().map(|(_, command)| command)
    }
}
