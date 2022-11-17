use bumpalo::Bump;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use rodeo::Rodeo;
use typed_arena::Arena as TypedArena;

struct ToDrop<T>(T);

impl<T> Drop for ToDrop<T> {
    fn drop(&mut self) {
        black_box(())
    }
}

fn comparison(c: &mut Criterion) {
    assert_eq!(
        rodeo::HEADER_LAYOUT.size(),
        std::mem::size_of::<(usize, usize)>()
    );

    let mut group = c.benchmark_group("typed_vs_rodeo");
    let block = 500_usize;
    for i in 0..10 {
        let size = i * block;
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("bumpalo", size), &size, |b, &size| {
            b.iter(|| with_bumpalo(size))
        });
        group.bench_with_input(BenchmarkId::new("typed_arena", size), &size, |b, &size| {
            b.iter(|| with_typed_arena(size))
        });
        group.bench_with_input(BenchmarkId::new("rodeo", size), &size, |b, &size| {
            b.iter(|| with_rodeo(size))
        });
        group.bench_with_input(
            BenchmarkId::new("rodeo_no_drop", size),
            &size,
            |b, &size| b.iter(|| with_rodeo_no_drop(size)),
        );
        group.bench_with_input(
            BenchmarkId::new("rodeo_no_drop_need_drop", size),
            &size,
            |b, &size| b.iter(|| with_rodeo_no_drop_need_drop(size)),
        );
    }
    group.finish();
}

fn with_bumpalo(n: usize) {
    let arena = Bump::new();
    for i in 0..n {
        arena.alloc(ToDrop(i));
    }
    let _ = black_box(arena);
}

fn with_typed_arena(n: usize) {
    let arena = TypedArena::new();
    for i in 0..n {
        arena.alloc(ToDrop(i));
    }
    let _ = black_box(arena);
}

fn with_rodeo(n: usize) {
    let arena = Rodeo::new();
    for i in 0..n {
        arena.alloc(ToDrop(i));
    }
    let _ = black_box(arena);
}

fn with_rodeo_no_drop_need_drop(n: usize) {
    let arena = Rodeo::new();
    for i in 0..n {
        arena.alloc(ToDrop(i));
    }
    arena.leak_all();
    let _ = black_box(arena);
}

fn with_rodeo_no_drop(n: usize) {
    let arena = Rodeo::new();
    for i in 0..n {
        arena.alloc(i);
    }
    let _ = black_box(arena);
}

criterion_group!(benches, comparison);
criterion_main!(benches);
