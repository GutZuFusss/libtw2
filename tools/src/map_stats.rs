use df;
use logger;
use map;
use std::collections::HashMap;
use std::env;
use std::io;
use std::path::Path;

#[derive(Default)]
struct ErrorStats {
    map_errors: HashMap<map::format::Error,u64>,
    df_errors: HashMap<df::format::Error,u64>,
    io_errors: Vec<io::Error>,
    ok: u64,
}

fn update_error_stats(stats: &mut ErrorStats, err: map::Error) {
    match err {
        map::Error::Map(e) => {
            *stats.map_errors.entry(e).or_insert(0) += 1;
        }
        map::Error::Df(df::Error::Df(e)) => {
            *stats.df_errors.entry(e).or_insert(0) += 1;
        }
        map::Error::Df(df::Error::Io(e)) => {
            stats.io_errors.push(e);
        }
    }
}

fn print_error_stats(error_stats: &ErrorStats) {
    for (e, c) in &error_stats.map_errors {
        println!("{:?}: {}", e, c);
    }
    for (e, c) in &error_stats.df_errors {
        println!("{:?}: {}", e, c);
    }
    for e in &error_stats.io_errors {
        println!("{:?}", e);
    }
    println!("ok: {}", error_stats.ok);
}

fn process<D, P>(path: &Path, process_inner: P, stats: &mut D) -> Result<(), map::Error>
    where P: FnOnce(&Path, df::Reader, &mut D) -> Result<(), map::Error>,
{
    let reader = try!(df::Reader::open(path));
    process_inner(path, reader, stats)
}

pub fn stats<D, P, S>(mut process_inner: P, summary: S)
    where D: Default,
          P: FnMut(&Path, df::Reader, &mut D) -> Result<(), map::Error>,
          S: FnOnce(&D),
{
    logger::init();

    let mut args = env::args_os();
    let mut have_args = false;
    let program_name = args.next().unwrap();

    let mut error_stats = ErrorStats::default();
    let mut stats = D::default();
    for arg in args {
        have_args = true;
        match process(Path::new(&arg), &mut process_inner, &mut stats) {
            Ok(()) => error_stats.ok += 1,
            Err(err) => {
                println!("{}: {:?}", arg.to_string_lossy(), err);
                update_error_stats(&mut error_stats, err);
            }
        }
    }
    if !have_args {
        println!("USAGE: {} <MAP>...", program_name.to_string_lossy());
        return;
    }
    print_error_stats(&error_stats);
    println!("--------");
    summary(&stats);
}
