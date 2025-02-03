use std::collections::LinkedList;

use criterion::{criterion_group, criterion_main, Criterion};
use cacheus::ArenaLinkedList;

fn benchmark(c: &mut Criterion)
{
    let mut group = c.benchmark_group(format!("ArenaLinkedList"));

    group.bench_function("Add first", |b| {
        let mut list = ArenaLinkedList::new_with_capacity(4);
        b.iter(|| list.add_first(1));
    });

    group.bench_function("Add last", |b| {
        let mut list = ArenaLinkedList::new_with_capacity(4);
        b.iter(|| list.add_last(1));
    });

    group.bench_function("Remove", |b| {
        let mut list = ArenaLinkedList::new_with_capacity(4);
        let index = list.add_first(1).unwrap();
        b.iter(|| list.remove(index));
    });

    group.finish();

    let mut group = c.benchmark_group(format!("LinkedList"));

    group.bench_function("Add first", |b| {
        let mut list = LinkedList::new();
        b.iter(|| list.push_front(1));
    });

    group.bench_function("Add last", |b| {
        let mut list = LinkedList::new();
        b.iter(|| list.push_back(1));
    });

    group.bench_function("Remove", |b| {
        let mut list = LinkedList::new();
        list.push_front(1);
        b.iter(|| list.pop_front());
    });

    group.finish();
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
