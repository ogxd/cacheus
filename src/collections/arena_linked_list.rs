use serde::{Serialize, Deserialize};

#[allow(dead_code)]
#[derive(Serialize, Deserialize)]
pub struct ArenaLinkedList<T>
{
    count: usize,
    first_index: usize,
    last_index: usize,
    first_free_node_index: usize,
    array: Vec<ArenaLinkedListNode<T>>,
}

#[allow(dead_code)]
impl<T> ArenaLinkedList<T>
{
    pub fn new_with_capacity(capacity: usize) -> Self
    {
        let capacity = capacity.min(1);
        let mut array = Vec::with_capacity(capacity);
        array.resize_with(capacity, || ArenaLinkedListNode::new());
        let mut new = Self {
            count: 0,
            first_index: usize::MAX,
            last_index: usize::MAX,
            first_free_node_index: 0,
            array: array,
        };
        new.clear();
        new
    }

    pub fn count(&self) -> usize
    {
        self.count
    }

    pub fn get_first_index(&self) -> Result<usize, ()>
    {
        if self.count == 0 {
            return Err(());
        }
        Ok(self.first_index)
    }

    pub fn get_last_index(&self) -> Result<usize, ()>
    {
        if self.count == 0 {
            return Err(());
        }
        Ok(self.last_index)
    }

    pub fn clear(&mut self)
    {
        self.fill_free(0, self.array.len());
        self.count = 0;
        self.first_index = usize::MAX;
        self.last_index = usize::MAX;
    }

    fn fill_free(&mut self, start: usize, count: usize)
    {
        self.first_free_node_index = start;
        for i in start..(start + count - 1) {
            self.array[i].after_index = i + 1;
        }
        self.array[start + count - 1].after_index = usize::MAX;
    }

    pub fn get(&self, index: usize) -> Result<&ArenaLinkedListNode<T>, ()>
    {
        let node = &self.array[index];
        if node.value.is_none() {
            return Err(());
        }
        Ok(node)
    }

    pub fn remove(&mut self, index: usize) -> Result<(), ()>
    {
        if self.count == 0 || index == usize::MAX || index > self.array.len() {
            return Err(());
        }
        let node = &mut self.array[index];

        if node.value.is_none() {
            return Err(());
        }

        let node_after = node.after_index;
        let node_before = node.before_index;

        // Mark node as free
        node.value = None; // Free reference
        node.before_index = usize::MAX;
        node.after_index = self.first_free_node_index;
        self.first_free_node_index = index;

        // Remap links
        if node_before == usize::MAX {
            self.first_index = node_after;
        } else {
            let before_node = &mut self.array[node_before]; // cannot borrow `self.array` as mutable more than once at a time
            before_node.after_index = node_after;
        }

        if node_after == usize::MAX {
            self.last_index = node_before;
        } else {
            let after_node = &mut self.array[node_after]; // cannot borrow `self.array` as mutable more than once at a time
            after_node.before_index = node_before;
        }

        // Decrement count
        self.count -= 1;

        Ok(())
    }

    pub fn add_before(&mut self, value: T, index: usize) -> Result<usize, &'static str>
    {
        // Create new node
        let new_node_index = self.create_node(value);

        // Remap links
        if self.count == 0 || index == usize::MAX {
            self.first_index = new_node_index;
            self.last_index = new_node_index;
        } else {
            if self.array[index].value.is_none() {
                return Err("This index does not refer to a valid entry");
            }

            let node = &self.array[index];
            let node_before_index = node.before_index;

            let new_node = &mut self.array[new_node_index];
            new_node.before_index = node_before_index;
            new_node.after_index = index;

            if node_before_index != usize::MAX {
                let node_before = &mut self.array[node_before_index];
                node_before.after_index = new_node_index;
            }
            let node = &mut self.array[index];
            node.before_index = new_node_index;

            // If inserted before first, it becomes first
            if self.first_index == index {
                self.first_index = new_node_index;
            }
        }

        Ok(new_node_index)
    }

    pub fn add_first(&mut self, value: T) -> Result<usize, &'static str>
    {
        self.add_before(value, self.first_index)
    }

    pub fn add_after(&mut self, value: T, index: usize) -> Result<usize, &'static str>
    {
        // Create new node
        let new_node_index = self.create_node(value);

        // Remap links
        if self.count == 0 || index == usize::MAX {
            self.first_index = new_node_index;
            self.last_index = new_node_index;
        } else {
            if self.array[index].value.is_none() {
                return Err("This index does not refer to a valid entry");
            }

            let node = &self.array[index];
            let node_after_index = node.after_index;

            let new_node = &mut self.array[new_node_index];
            new_node.before_index = index;
            new_node.after_index = node_after_index;

            if node_after_index != usize::MAX {
                let node_after = &mut self.array[node_after_index];
                node_after.before_index = new_node_index;
            }
            let node = &mut self.array[index];
            node.after_index = new_node_index;

            // If inserted after last, it becomes last
            if self.last_index == index {
                self.last_index = new_node_index;
            }
        }

        Ok(new_node_index)
    }

    pub fn add_last(&mut self, value: T) -> Result<usize, &'static str>
    {
        self.add_after(value, self.last_index)
    }

    fn create_node(&mut self, value: T) -> usize
    {
        if self.array.len() == self.count {
            let len = self.array.len();
            let a = &mut self.array;
            a.resize_with(len * 2, || ArenaLinkedListNode::new());

            self.fill_free(self.count, self.array.len() - self.count);
        }

        let index = self.first_free_node_index;

        let new_node = &mut self.array[index];

        self.first_free_node_index = new_node.after_index;
        new_node.after_index = usize::MAX;
        new_node.before_index = usize::MAX;
        new_node.value = Some(value);

        self.count += 1;

        index
    }

    pub fn iter(&self) -> Enumerator<T>
    {
        Enumerator {
            list: self,
            index: self.first_index,
        }
    }
}

pub struct Enumerator<'a, T>
{
    list: &'a ArenaLinkedList<T>,
    index: usize,
}

impl<'a, T> Iterator for Enumerator<'a, T>
{
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item>
    {
        if self.index == usize::MAX {
            return None;
        } else {
            let node = self.list.get(self.index).unwrap();
            self.index = node.after_index;
            return node.value.as_ref();
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct ArenaLinkedListNode<T>
{
    before_index: usize,
    after_index: usize,
    value: Option<T>,
}

impl<T> ArenaLinkedListNode<T>
{
    pub fn new() -> Self
    {
        Self {
            before_index: usize::MAX,
            after_index: usize::MAX,
            value: None,
        }
    }

    pub fn get_after_index(&self) -> usize
    {
        self.after_index
    }

    pub fn get_value(&self) -> &Option<T>
    {
        &self.value
    }
}

#[cfg(test)]
mod tests
{
    use itertools::Itertools;

    use super::*;

    #[test]
    fn test_arena_linked_list()
    {
        let mut list = ArenaLinkedList::<&str>::new_with_capacity(10);
        assert_eq!(list.count(), 0);

        // Add "hello"
        let hello_index = list.add_first("hello").expect("Failed adding hello");
        assert_eq!(list.count(), 1);

        // Add "world" last
        let world_index = list.add_last("world").expect("Failed adding world");
        assert_eq!(list.count(), 2);

        // Insert "wonderful" before "world"
        let wonderful_index = list
            .add_before("wonderful", world_index)
            .expect("Failed adding wonderful");
        assert_eq!(list.count(), 3);

        // Get
        let hello_node = list.get(hello_index).expect("Failed getting hello");
        assert_eq!(hello_node.value.unwrap(), "hello");

        // Iterate
        let content = list.iter().join(" ");
        println!("-> {}", content);
        assert_eq!(content, "hello wonderful world");

        // Remove "hello"
        list.remove(hello_index).expect("Failed removing hello");
        assert_eq!(list.count(), 2);
        assert!(list.remove(hello_index).is_err());

        // Remove "wonderful"
        list.remove(wonderful_index).expect("Failed removing wonderful");
        assert_eq!(list.count(), 1);
        assert!(list.remove(wonderful_index).is_err());

        // Remove "world"
        list.remove(world_index).expect("Failed removing world");
        assert_eq!(list.count(), 0);
        assert!(list.remove(world_index).is_err());
    }
}
