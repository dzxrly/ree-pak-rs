//! list 文件加载性能基准测试

use std::{fs, hint::black_box, path::PathBuf};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use ree_pak_core::FileNameTable;

const FIXTURES: &[(&str, &str)] = &[
    ("small", "DD2CCS_PC_Demo.list"),
    ("medium", "MHR_PC_Release.list"),
    ("large", "MHWs_STM_Release.list"),
];

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("assets")
        .join("filelist_raw")
        .join(name)
}

fn bench_filelist_load(c: &mut Criterion) {
    let mut from_file = c.benchmark_group("filelist/from_list_file");
    from_file.sample_size(10);

    for &(label, file_name) in FIXTURES {
        let path = fixture_path(file_name);
        let size = fs::metadata(&path)
            .unwrap_or_else(|err| panic!("failed to read fixture metadata `{}`: {err}", path.display()))
            .len();
        from_file.throughput(Throughput::Bytes(size));
        from_file.bench_with_input(BenchmarkId::new("load", label), &path, |b, path| {
            b.iter(|| black_box(FileNameTable::from_list_file(path).unwrap()));
        });
    }
    from_file.finish();
}

criterion_group!(benches, bench_filelist_load);
criterion_main!(benches);
