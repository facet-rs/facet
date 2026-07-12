use std::hint::black_box;
use std::time::Instant;

use weavy::mem::Layout;
use weavy::task::{
    Fn as TaskFn, FnId, HostFn, Op, Program, Task, TaskStep, ValueMemories, ValueMemory,
};

const ELEM_SCHEMA_REF: i64 = 0x6172_7261_795f_6936;
const ARRAY_WORDS: usize = 1024;

fn main() {
    let iters = std::env::var("WEAVY_ARRAY_GET_BENCH_ITERS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(200_000);
    let payload = array_payload(ARRAY_WORDS);
    let memories = [ValueMemory::from_slice(&payload)];
    let native = native_program();
    let host = host_program();

    measure("interp native-get", iters, || {
        run_interp_native(&native, &memories, iters)
    });
    measure("interp host-get", iters, || {
        run_interp_host(&host, &payload, iters)
    });

    #[cfg(weavy_jit_active)]
    {
        use weavy::jit::task_lane::JitProgram;

        if let (Some(native_jit), Some(host_jit)) =
            (JitProgram::compile(&native), JitProgram::compile(&host))
        {
            measure("jit native-get", iters, || {
                run_jit_native(&native_jit, &memories, iters)
            });
            measure("jit host-get", iters, || {
                run_jit_host(&host_jit, &payload, iters)
            });
        }
    }
}

fn measure(label: &str, iters: usize, mut f: impl FnMut() -> i64) {
    let start = Instant::now();
    let sum = f();
    let elapsed = start.elapsed();
    let ns_per = elapsed.as_nanos() as f64 / iters as f64;
    println!("{label}: {ns_per:.2} ns/get, checksum={sum}");
}

fn frame() -> Layout {
    Layout { size: 32, align: 8 }
}

fn native_program() -> Program {
    Program {
        fns: vec![TaskFn {
            frame: frame(),
            code: vec![
                Op::ConstI64 { dst: 0, value: 0 },
                Op::LoadArrayWord {
                    dst: 16,
                    present: 24,
                    array: 0,
                    index: 8,
                    elem_schema_ref: ELEM_SCHEMA_REF,
                },
                Op::Ret { src: 16, size: 8 },
            ],
        }],
    }
}

fn host_program() -> Program {
    Program {
        fns: vec![TaskFn {
            frame: frame(),
            code: vec![
                Op::ConstI64 { dst: 0, value: 0 },
                Op::HostCall { host: 0 },
                Op::Ret { src: 16, size: 8 },
            ],
        }],
    }
}

fn run_interp_native(program: &Program, memories: &[ValueMemory], iters: usize) -> i64 {
    let mut sum = 0;
    for i in 0..iters {
        let mut task = Task::spawn(program, FnId(0));
        task.write_i64(8, (i % ARRAY_WORDS) as i64);
        assert_eq!(
            task.run_hosted_with_value_memories(
                program,
                &mut [],
                &[],
                &mut [],
                ValueMemories {
                    store: memories,
                    molten: &[],
                },
            ),
            TaskStep::Done
        );
        sum += black_box(task.result_i64());
    }
    sum
}

fn run_interp_host(program: &Program, payload: &[u8], iters: usize) -> i64 {
    let mut sum = 0;
    for i in 0..iters {
        let mut task = Task::spawn(program, FnId(0));
        task.write_i64(8, (i % ARRAY_WORDS) as i64);
        let mut host = |frame: &mut [u8]| host_get(frame, payload);
        let mut hosts: [HostFn<'_>; 1] = [&mut host];
        drive_interp_host(&mut task, program, &mut hosts);
        sum += black_box(task.result_i64());
    }
    sum
}

#[cfg(weavy_jit_active)]
fn run_jit_native(
    program: &weavy::jit::task_lane::JitProgram,
    memories: &[ValueMemory],
    iters: usize,
) -> i64 {
    use weavy::jit::task_lane::JitTask;

    let mut sum = 0;
    for i in 0..iters {
        let mut task = JitTask::spawn(program, FnId(0));
        task.write_i64(8, (i % ARRAY_WORDS) as i64);
        assert_eq!(
            task.run_hosted_with_value_memories(
                program,
                &mut [],
                &[],
                &mut [],
                ValueMemories {
                    store: memories,
                    molten: &[],
                },
            ),
            TaskStep::Done
        );
        sum += black_box(task.result_i64());
    }
    sum
}

#[cfg(weavy_jit_active)]
fn run_jit_host(program: &weavy::jit::task_lane::JitProgram, payload: &[u8], iters: usize) -> i64 {
    use weavy::jit::task_lane::JitTask;

    let mut sum = 0;
    for i in 0..iters {
        let mut task = JitTask::spawn(program, FnId(0));
        task.write_i64(8, (i % ARRAY_WORDS) as i64);
        let mut host = |frame: &mut [u8]| host_get(frame, payload);
        let mut hosts: [HostFn<'_>; 1] = [&mut host];
        drive_jit_host(&mut task, program, &mut hosts);
        sum += black_box(task.result_i64());
    }
    sum
}

fn drive_interp_host(task: &mut Task, program: &Program, hosts: &mut [HostFn<'_>]) {
    loop {
        match task.run_hosted(program, &mut [], &[], hosts) {
            TaskStep::Done => return,
            TaskStep::Yielded => {}
            TaskStep::Parked { input } => panic!("bench program parked on input {input}"),
        }
    }
}

#[cfg(weavy_jit_active)]
fn drive_jit_host(
    task: &mut weavy::jit::task_lane::JitTask,
    program: &weavy::jit::task_lane::JitProgram,
    hosts: &mut [HostFn<'_>],
) {
    loop {
        match task.run_hosted(program, &mut [], &[], hosts) {
            TaskStep::Done => return,
            TaskStep::Yielded => {}
            TaskStep::Parked { input } => panic!("bench program parked on input {input}"),
        }
    }
}

fn array_payload(words: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(24 + words * 8);
    bytes.extend_from_slice(&0i64.to_le_bytes());
    bytes.extend_from_slice(&ELEM_SCHEMA_REF.to_le_bytes());
    bytes.extend_from_slice(&(words as i64).to_le_bytes());
    for i in 0..words {
        bytes.extend_from_slice(&(i as i64).to_le_bytes());
    }
    bytes
}

fn host_get(frame: &mut [u8], payload: &[u8]) {
    let index = read_i64(frame, 8);
    let value = if index < 0 {
        0
    } else {
        let index = index as usize;
        if index >= ARRAY_WORDS {
            0
        } else {
            read_i64(payload, 24 + index * 8)
        }
    };
    frame[16..24].copy_from_slice(&value.to_le_bytes());
}

fn read_i64(bytes: &[u8], offset: usize) -> i64 {
    i64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("i64 bytes"))
}
