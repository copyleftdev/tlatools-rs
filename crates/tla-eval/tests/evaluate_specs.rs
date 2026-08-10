//! The evaluator is judged on the specifications it exists to serve, and on the
//! mistakes those specifications are meant to catch. Each rejection test below
//! reproduces the illegal transition one of the benchmark's mutants makes.

mod support;

use std::collections::BTreeMap;
use support::{evaluator, n, rec, s, seq, set, spec, st, strs};

use tla_eval::Value;

// ------------------------------------------------------------ bounded buffer

fn buffer_state(buf: Value, next_val: i64, received: Value) -> tla_eval::State {
    st(&[
        ("buf", buf),
        ("nextVal", n(next_val)),
        ("received", received),
    ])
}

#[test]
fn bounded_buffer_accepts_the_protocol() {
    let m = spec("BoundedBuffer");
    let e = evaluator(&m, &[("Capacity", n(2)), ("MaxItems", n(3))]);

    let empty = buffer_state(seq([]), 1, seq([]));
    assert!(e.holds_at("Init", &empty).unwrap());
    assert!(e.holds_at("TypeOK", &empty).unwrap());

    let one = buffer_state(seq([n(1)]), 2, seq([]));
    assert!(e.step_allowed("Next", &empty, &one).unwrap(), "Put");

    let taken = buffer_state(seq([]), 2, seq([n(1)]));
    assert!(e.step_allowed("Next", &one, &taken).unwrap(), "Get");
}

/// Mutant `m01_lifo`: taking from the tail instead of the head.
#[test]
fn bounded_buffer_rejects_lifo() {
    let m = spec("BoundedBuffer");
    let e = evaluator(&m, &[("Capacity", n(2)), ("MaxItems", n(3))]);

    let full = buffer_state(seq([n(1), n(2)]), 3, seq([]));
    let lifo = buffer_state(seq([n(1)]), 3, seq([n(2)]));
    assert!(!e.step_allowed("Next", &full, &lifo).unwrap());

    let fifo = buffer_state(seq([n(2)]), 3, seq([n(1)]));
    assert!(e.step_allowed("Next", &full, &fifo).unwrap());
}

/// Mutant `m02_off_by_one_capacity`: one more item than the buffer holds.
#[test]
fn bounded_buffer_rejects_overfilling() {
    let m = spec("BoundedBuffer");
    let e = evaluator(&m, &[("Capacity", n(2)), ("MaxItems", n(3))]);

    let full = buffer_state(seq([n(1), n(2)]), 3, seq([]));
    let over = buffer_state(seq([n(1), n(2), n(3)]), 4, seq([]));
    assert!(!e.step_allowed("Next", &full, &over).unwrap());
}

// --------------------------------------------------------- two-phase commit

fn prepared_msg(rm: &str) -> Value {
    rec(&[("type", s("Prepared")), ("rm", s(rm))])
}

fn tp_state(rm_state: Value, tm_state: &str, prepared: Value, msgs: Value) -> tla_eval::State {
    st(&[
        ("rmState", rm_state),
        ("tmState", s(tm_state)),
        ("tmPrepared", prepared),
        ("msgs", msgs),
    ])
}

#[test]
fn two_phase_accepts_prepare_and_abort() {
    let m = spec("TwoPhase");
    let e = evaluator(&m, &[("RM", strs(["r1", "r2"]))]);

    let init = tp_state(
        rec(&[("r1", s("working")), ("r2", s("working"))]),
        "init",
        set([]),
        set([]),
    );
    assert!(e.holds_at("TPInit", &init).unwrap());
    assert!(e.holds_at("TPTypeOK", &init).unwrap());

    let one_prepared = tp_state(
        rec(&[("r1", s("prepared")), ("r2", s("working"))]),
        "init",
        set([]),
        set([prepared_msg("r1")]),
    );
    assert!(e.step_allowed("TPNext", &init, &one_prepared).unwrap());

    let aborted = tp_state(
        rec(&[("r1", s("prepared")), ("r2", s("working"))]),
        "done",
        set([]),
        set([prepared_msg("r1"), rec(&[("type", s("Abort"))])]),
    );
    assert!(e.step_allowed("TPNext", &one_prepared, &aborted).unwrap());
}

/// Mutant `m01_commit_without_all_prepared`.
#[test]
fn two_phase_rejects_commit_before_every_rm_prepared() {
    let m = spec("TwoPhase");
    let e = evaluator(&m, &[("RM", strs(["r1", "r2"]))]);

    let one_prepared = tp_state(
        rec(&[("r1", s("prepared")), ("r2", s("working"))]),
        "init",
        set([s("r1")]),
        set([prepared_msg("r1")]),
    );
    let committed = tp_state(
        rec(&[("r1", s("prepared")), ("r2", s("working"))]),
        "done",
        set([s("r1")]),
        set([prepared_msg("r1"), rec(&[("type", s("Commit"))])]),
    );
    assert!(!e.step_allowed("TPNext", &one_prepared, &committed).unwrap());

    let both = tp_state(
        rec(&[("r1", s("prepared")), ("r2", s("prepared"))]),
        "init",
        strs(["r1", "r2"]),
        set([prepared_msg("r1"), prepared_msg("r2")]),
    );
    let legal = tp_state(
        rec(&[("r1", s("prepared")), ("r2", s("prepared"))]),
        "done",
        strs(["r1", "r2"]),
        set([
            prepared_msg("r1"),
            prepared_msg("r2"),
            rec(&[("type", s("Commit"))]),
        ]),
    );
    assert!(e.step_allowed("TPNext", &both, &legal).unwrap());
}

// -------------------------------------------------------------------- raft

fn raft_state(
    terms: [i64; 3],
    roles: [&str; 3],
    voted: [&str; 3],
    granted: [Value; 3],
) -> tla_eval::State {
    let by_server = |values: [Value; 3]| {
        rec(&[
            ("s1", values[0].clone()),
            ("s2", values[1].clone()),
            ("s3", values[2].clone()),
        ])
    };
    st(&[
        ("currentTerm", by_server(terms.map(n))),
        ("role", by_server(roles.map(s))),
        ("votedFor", by_server(voted.map(s))),
        ("votesGranted", by_server(granted)),
    ])
}

fn raft_init() -> tla_eval::State {
    raft_state(
        [0, 0, 0],
        ["follower"; 3],
        ["none"; 3],
        [set([]), set([]), set([])],
    )
}

#[test]
fn raft_accepts_an_election() {
    let m = spec("RaftElection");
    let e = evaluator(
        &m,
        &[("Server", strs(["s1", "s2", "s3"])), ("MaxTerm", n(2))],
    );

    let init = raft_init();
    assert!(e.holds_at("Init", &init).unwrap());
    assert!(e.holds_at("TypeOK", &init).unwrap());

    let candidate = raft_state(
        [1, 0, 0],
        ["candidate", "follower", "follower"],
        ["s1", "none", "none"],
        [strs(["s1"]), set([]), set([])],
    );
    assert!(
        e.step_allowed("Next", &init, &candidate).unwrap(),
        "Timeout"
    );
}

/// Mutant `m01_no_majority_required`: one vote out of three is not a majority.
#[test]
fn raft_rejects_a_leader_without_a_majority() {
    let m = spec("RaftElection");
    let e = evaluator(
        &m,
        &[("Server", strs(["s1", "s2", "s3"])), ("MaxTerm", n(2))],
    );

    let one_vote = raft_state(
        [1, 0, 0],
        ["candidate", "follower", "follower"],
        ["s1", "none", "none"],
        [strs(["s1"]), set([]), set([])],
    );
    let crowned = raft_state(
        [1, 0, 0],
        ["leader", "follower", "follower"],
        ["s1", "none", "none"],
        [strs(["s1"]), set([]), set([])],
    );
    assert!(!e.step_allowed("Next", &one_vote, &crowned).unwrap());

    let two_votes = raft_state(
        [1, 1, 0],
        ["candidate", "follower", "follower"],
        ["s1", "s1", "none"],
        [strs(["s1", "s2"]), set([]), set([])],
    );
    let legal = raft_state(
        [1, 1, 0],
        ["leader", "follower", "follower"],
        ["s1", "s1", "none"],
        [strs(["s1", "s2"]), set([]), set([])],
    );
    assert!(e.step_allowed("Next", &two_votes, &legal).unwrap());
}

/// Mutant `m02_votes_more_than_once`: a server has one vote per term.
#[test]
fn raft_rejects_a_second_vote_in_the_same_term() {
    let m = spec("RaftElection");
    let e = evaluator(
        &m,
        &[("Server", strs(["s1", "s2", "s3"])), ("MaxTerm", n(2))],
    );

    let s2_has_voted = raft_state(
        [1, 1, 1],
        ["candidate", "follower", "candidate"],
        ["s1", "s1", "s3"],
        [strs(["s1", "s2"]), set([]), strs(["s3"])],
    );
    let votes_again = raft_state(
        [1, 1, 1],
        ["candidate", "follower", "candidate"],
        ["s1", "s3", "s3"],
        [strs(["s1", "s2"]), set([]), strs(["s3", "s2"])],
    );
    assert!(!e.step_allowed("Next", &s2_has_voted, &votes_again).unwrap());
}

// ------------------------------------------------------------ bank transfer

#[test]
fn bank_transfer_conserves_and_refuses_overdraft() {
    let m = spec("BankTransfer");
    let e = evaluator(
        &m,
        &[
            ("Acct", strs(["a", "b"])),
            ("Amounts", set([n(1), n(2)])),
            ("InitBal", n(5)),
        ],
    );

    let init = st(&[("bal", rec(&[("a", n(5)), ("b", n(5))]))]);
    assert!(e.holds_at("Init", &init).unwrap());
    // Conserved is RECURSIVE and uses CHOOSE; both must evaluate.
    assert!(e.holds_at("Conserved", &init).unwrap());
    assert_eq!(e.value_of("Total", &init).unwrap(), n(10));

    let moved = st(&[("bal", rec(&[("a", n(3)), ("b", n(7))]))]);
    assert!(e.step_allowed("Next", &init, &moved).unwrap());

    let nearly_empty = st(&[("bal", rec(&[("a", n(1)), ("b", n(9))]))]);
    let overdrawn = st(&[("bal", rec(&[("a", n(-1)), ("b", n(11))]))]);
    assert!(!e.step_allowed("Next", &nearly_empty, &overdrawn).unwrap());
}

// ------------------------------------------------------------ ring election

fn ring(m: &tla_eval::Spec) -> tla_eval::Evaluator<'_> {
    evaluator(
        m,
        &[
            ("Node", strs(["n1", "n2", "n3"])),
            ("Ident", rec(&[("n1", n(1)), ("n2", n(3)), ("n3", n(2))])),
            (
                "Succ",
                rec(&[("n1", s("n2")), ("n2", s("n3")), ("n3", s("n1"))]),
            ),
        ],
    )
}

fn ring_init() -> tla_eval::State {
    st(&[
        (
            "inbox",
            rec(&[
                ("n1", set([n(2)])),
                ("n2", set([n(1)])),
                ("n3", set([n(3)])),
            ]),
        ),
        ("leader", set([])),
    ])
}

/// `Init` here is a set comprehension over a set comprehension, and `MaxId` is
/// a `CHOOSE`; both must produce the same value every time they are evaluated.
#[test]
fn ring_election_builds_its_initial_state() {
    let m = spec("RingElection");
    let e = ring(&m);
    let init = ring_init();
    assert!(e.holds_at("Init", &init).unwrap());
    assert!(e.holds_at("TypeOK", &init).unwrap());
    assert_eq!(e.value_of("MaxId", &init).unwrap(), n(3));
    assert_eq!(e.value_of("MaxId", &init).unwrap(), n(3));
}

/// Mutant `m01_forwards_smaller_ids`: an identifier below your own is dropped,
/// never passed on.
#[test]
fn ring_election_rejects_forwarding_a_smaller_id() {
    let m = spec("RingElection");
    let e = ring(&m);
    let init = ring_init();

    let forwarded = st(&[
        (
            "inbox",
            rec(&[
                ("n1", set([n(2)])),
                ("n2", set([])),
                ("n3", set([n(3), n(1)])),
            ]),
        ),
        ("leader", set([])),
    ]);
    assert!(!e.step_allowed("Next", &init, &forwarded).unwrap());

    let discarded = st(&[
        (
            "inbox",
            rec(&[("n1", set([n(2)])), ("n2", set([])), ("n3", set([n(3)]))]),
        ),
        ("leader", set([])),
    ]);
    assert!(e.step_allowed("Next", &init, &discarded).unwrap());
}

// ------------------------------------------------------------------- paxos

fn paxos_evaluator(m: &tla_eval::Spec) -> tla_eval::Evaluator<'_> {
    evaluator(
        m,
        &[
            ("Acceptor", strs(["a1", "a2", "a3"])),
            ("Value", strs(["v1", "v2"])),
            ("Ballot", set([n(0), n(1)])),
            (
                "Quorum",
                set([strs(["a1", "a2"]), strs(["a1", "a3"]), strs(["a2", "a3"])]),
            ),
        ],
    )
}

fn paxos_state(msgs: Value) -> tla_eval::State {
    let all = |v: Value| rec(&[("a1", v.clone()), ("a2", v.clone()), ("a3", v)]);
    st(&[
        ("maxBal", all(n(-1))),
        ("maxVBal", all(n(-1))),
        ("maxVal", all(s("none"))),
        ("msgs", msgs),
    ])
}

#[test]
fn paxos_accepts_a_first_phase() {
    let m = spec("Paxos");
    let e = paxos_evaluator(&m);

    let init = paxos_state(set([]));
    assert!(e.holds_at("Init", &init).unwrap());
    assert!(e.holds_at("TypeOK", &init).unwrap());

    let announced = paxos_state(set([rec(&[("type", s("1a")), ("bal", n(0))])]));
    assert!(e.step_allowed("Next", &init, &announced).unwrap());
}

/// The rule the whole protocol turns on: a proposer may not propose at a ballot
/// until a quorum has reported, because only then is a value safe.
#[test]
fn paxos_rejects_a_proposal_without_a_quorum() {
    let m = spec("Paxos");
    let e = paxos_evaluator(&m);

    let init = paxos_state(set([]));
    let proposed = paxos_state(set([rec(&[
        ("type", s("2a")),
        ("bal", n(0)),
        ("val", s("v1")),
    ])]));
    assert!(!e.step_allowed("Next", &init, &proposed).unwrap());
}

// ------------------------------------------------------------- error paths

#[test]
fn an_action_evaluated_as_a_predicate_says_so() {
    let m = spec("BoundedBuffer");
    let e = evaluator(&m, &[("Capacity", n(2)), ("MaxItems", n(3))]);
    let err = e
        .holds_at("Next", &buffer_state(seq([]), 1, seq([])))
        .expect_err("Next primes its variables");
    assert!(matches!(err, tla_eval::Error::NoNextState(_)), "{err}");
}

#[test]
fn a_temporal_formula_is_refused_rather_than_guessed() {
    let m = spec("BoundedBuffer");
    let e = evaluator(&m, &[("Capacity", n(2)), ("MaxItems", n(3))]);
    let err = e
        .holds_at("Spec", &buffer_state(seq([]), 1, seq([])))
        .expect_err("Spec is a temporal formula");
    assert!(matches!(err, tla_eval::Error::NotGround(_)), "{err}");
}

#[test]
fn a_missing_constant_is_reported_before_evaluation() {
    let m = spec("BoundedBuffer");
    let err =
        tla_eval::Evaluator::new(&m, BTreeMap::new()).expect_err("both constants are missing");
    assert!(format!("{err}").contains("Capacity"), "{err}");
}
