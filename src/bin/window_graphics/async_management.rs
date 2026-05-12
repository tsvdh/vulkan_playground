use std::sync::mpsc::channel;
use std::thread;
use std::time::{Duration, Instant};
use crate::{AsyncManagement, AsynchronousTask, TimingItems};

impl AsyncManagement {
    pub fn new(timing_items: &TimingItems) -> Self {
        let (async_logic_prod, async_logic_cons) = channel::<()>();
        let (done_checker_prod, done_checker_cons) = channel::<()>();
        let (main_thread_prod, main_thread_cons) = channel::<(AsynchronousTask, Duration)>();

        thread::spawn(move || {
            loop {
                async_logic_cons.recv().unwrap();

                // expensive logic placeholder
                thread::sleep(Duration::from_micros(4200));

                done_checker_prod.send(()).unwrap();
            }
        });

        {
            let async_cpu_start = timing_items.async_cpu_start.clone();
            let render_gpu_start = timing_items.render_gpu_start.clone();
            let frame_render_end = timing_items.frame_render_end.clone();

            thread::spawn(move || {
                loop {
                    let message = done_checker_cons.try_recv();
                    if message.is_ok() {
                        let async_cpu_duration = Instant::now() - *async_cpu_start.lock().unwrap();
                        main_thread_prod.send((AsynchronousTask::CpuLogic, async_cpu_duration)).unwrap();
                    }

                    let mut frame_render_end_mutex = frame_render_end.lock().unwrap();
                    if frame_render_end_mutex.is_some() && frame_render_end_mutex.as_ref().unwrap().is_signaled().unwrap() {
                        let gpu_render_duration = Instant::now() - *render_gpu_start.lock().unwrap();
                        main_thread_prod.send((AsynchronousTask::GpuRender, gpu_render_duration)).unwrap();
                        *frame_render_end_mutex = None;
                    }
                    drop(frame_render_end_mutex);

                    thread::sleep(Duration::from_micros(50));
                }
            });
        }

        AsyncManagement {
            async_logic_prod,
            main_thread_cons,
        }
    }
}