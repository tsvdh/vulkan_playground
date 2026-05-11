use std::sync::mpsc::channel;
use std::thread;

fn main() {
    let (a, b) = channel::<()>();

    let handle = thread::spawn(move || {
        // a.send(()).unwrap();
        // a.send(()).unwrap();
    });

    handle.join().unwrap();

    for message in b.try_iter() {
        println!("new message");
    }

    // handle.join().unwrap();
}