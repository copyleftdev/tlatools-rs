---- MODULE RaftElection ----
EXTENDS Naturals, FiniteSets
CONSTANTS Server, MaxTerm
NoOne == "none"
VARIABLES currentTerm, role, votedFor, votesGranted
TypeOK == currentTerm \in [Server -> 0..MaxTerm] /\ role \in [Server -> {"follower", "candidate", "leader"}] /\ votedFor \in [Server -> Server \cup {NoOne}] /\ votesGranted \in [Server -> SUBSET Server]
Init == currentTerm = [s \in Server |-> 0] /\ role = [s \in Server |-> "follower"] /\ votedFor = [s \in Server |-> NoOne] /\ votesGranted = [s \in Server |-> {}]
Timeout(s) == role[s] \in {"follower", "candidate"} /\ currentTerm[s] < MaxTerm /\ currentTerm' = [currentTerm EXCEPT ![s] = @ + 1] /\ role' = [role EXCEPT ![s] = "candidate"] /\ votedFor' = [votedFor EXCEPT ![s] = s] /\ votesGranted' = [votesGranted EXCEPT ![s] = {s}]
UpdateTerm(v, c) == currentTerm[c] > currentTerm[v] /\ currentTerm' = [currentTerm EXCEPT ![v] = currentTerm[c]] /\ role' = [role EXCEPT ![v] = "follower"] /\ votedFor' = [votedFor EXCEPT ![v] = NoOne] /\ votesGranted' = [votesGranted EXCEPT ![v] = {}]
GrantVote(v, c) == v # c /\ role[c] = "candidate" /\ currentTerm[v] = currentTerm[c] /\ votedFor[v] = NoOne /\ votedFor' = [votedFor EXCEPT ![v] = c] /\ votesGranted' = [votesGranted EXCEPT ![c] = @ \cup {v}] /\ (UNCHANGED <<currentTerm, role>>)
BecomeLeader(c) == role[c] = "candidate" /\ Cardinality(votesGranted[c]) * 2 > Cardinality(Server) /\ role' = [role EXCEPT ![c] = "leader"] /\ (UNCHANGED <<currentTerm, votedFor, votesGranted>>)
Next == \E s \in Server : Timeout(s) \/ BecomeLeader(s) \/ (\E c \in Server : UpdateTerm(s, c) \/ GrantVote(s, c))
Spec == Init /\ ([][Next]_<<currentTerm, role, votedFor, votesGranted>>)
OneLeaderPerTerm == \A s1, s2 \in Server : role[s1] = "leader" /\ role[s2] = "leader" /\ currentTerm[s1] = currentTerm[s2] => s1 = s2
VotesAreReal == \A c \in Server : \A v \in votesGranted[c] : currentTerm[v] >= currentTerm[c]
====
