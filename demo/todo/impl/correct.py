"""A to-do list that does what the specification says.

The interface is three methods, and nothing here knows the specification
exists: `state()` says where we are, `actions()` says what the user could do
next, `step()` does one.
"""

IDS = ["a", "b"]


class Todo:
    def __init__(self):
        self.tasks = {i: "absent" for i in IDS}

    def state(self):
        return {"tasks": dict(self.tasks)}

    def actions(self):
        out = []
        for i in IDS:
            if self.tasks[i] == "absent":
                out.append(("add", i))
            if self.tasks[i] == "open":
                out.append(("complete", i))
            if self.tasks[i] == "done":
                out.append(("reopen", i))
            if self.tasks[i] != "absent":
                out.append(("delete", i))
        if any(v == "done" for v in self.tasks.values()):
            out.append(("clear_completed", None))
        return out

    def step(self, action):
        kind, i = action
        if kind == "add":
            self.tasks[i] = "open"
        elif kind == "complete":
            self.tasks[i] = "done"
        elif kind == "reopen":
            self.tasks[i] = "open"
        elif kind == "delete":
            self.tasks[i] = "absent"
        elif kind == "clear_completed":
            self.tasks = {
                k: ("absent" if v == "done" else v) for k, v in self.tasks.items()
            }
