use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::TryRecvError;
use std::{thread, time::Duration, time::Instant};
use uuid::Uuid;

pub struct Pomodoro {
    pub uuid: Uuid,
    duration: u64,
}

pub struct Break {
    pub uuid: Uuid,
    duration: u64,
}

impl Pomodoro {
    pub fn new(duration: u64) -> Self {
        Self {
            uuid: Uuid::new_v4(),
            duration: duration,
        }
    }

    pub fn run(&self) -> bool {
        match waiter(self.duration).recv() {
            Ok(cancelled) => {
                return !cancelled;
            }
            Err(_) => {
                println!("Error: not sure what happened");
                return false;
            }
        }
    }
}

impl Break {
    pub fn new(duration: u64) -> Self {
        Self {
            uuid: Uuid::new_v4(),
            duration: duration,
        }
    }

    pub fn run(&self) -> bool {
        match waiter(self.duration).recv() {
            Ok(cancelled) => {
                return !cancelled;
            }
            Err(_) => {
                println!("Error: not sure what happened");
                return false;
            }
        }
    }
}

fn waiter(duration: u64) -> Receiver<bool> {
    let (control_tx, control_rx) = channel();
    let (result_tx, result_rx) = channel::<bool>();

    ctrlc::set_handler(move || {
        control_tx
            .send(())
            .expect("Could not send signal on control channel.")
    })
    .expect("Error setting Ctrl-C handler");

    thread::spawn({
        move || {
            let mut done = false;
            let break_duration = Duration::new(60 * duration, 0);
            let start = Instant::now();

            while !done {
                if start.elapsed() > break_duration {
                    done = true;
                    result_tx.send(false).expect("could not send result");
                }

                match control_rx.try_recv() {
                    Ok(_) => {
                        done = true;
                        result_tx.send(true).expect("could not send result")
                    }
                    Err(TryRecvError::Disconnected) => {
                        println!("Error: channel disconnected");
                        done = true;
                    }
                    Err(TryRecvError::Empty) => thread::sleep(Duration::from_millis(25)),
                }
            }
        }
    })
    .join()
    .unwrap();
    return result_rx;
}
