use crate::CommandEnvelope;

#[derive(Debug, Clone)]
pub struct UndoRecord<S> {
    pub command: CommandEnvelope,
    pub before: S,
    pub after: S,
}

#[derive(Debug, Clone)]
pub struct CommandHistory<S> {
    done: Vec<UndoRecord<S>>,
    undone: Vec<UndoRecord<S>>,
    capacity: usize,
}

impl<S> Default for CommandHistory<S> {
    fn default() -> Self {
        Self {
            done: Vec::new(),
            undone: Vec::new(),
            capacity: 64,
        }
    }
}

impl<S> CommandHistory<S> {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            ..Self::default()
        }
    }

    pub fn push(&mut self, record: UndoRecord<S>) {
        self.done.push(record);
        self.undone.clear();
        if self.done.len() > self.capacity {
            self.done.remove(0);
        }
    }

    pub fn pop_undo(&mut self) -> Option<UndoRecord<S>> {
        self.done.pop()
    }

    pub fn push_redo(&mut self, record: UndoRecord<S>) {
        self.undone.push(record);
    }

    pub fn pop_redo(&mut self) -> Option<UndoRecord<S>> {
        self.undone.pop()
    }

    pub fn push_done_without_clearing_redo(&mut self, record: UndoRecord<S>) {
        self.done.push(record);
        if self.done.len() > self.capacity {
            self.done.remove(0);
        }
    }

    pub fn clear(&mut self) {
        self.done.clear();
        self.undone.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    pub fn len(&self) -> usize {
        self.done.len()
    }

    pub fn is_empty(&self) -> bool {
        self.done.is_empty()
    }

    pub fn undone_len(&self) -> usize {
        self.undone.len()
    }

    pub fn command_history(&self) -> &[UndoRecord<S>] {
        &self.done
    }
}
