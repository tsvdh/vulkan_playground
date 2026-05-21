use std::collections::VecDeque;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, Mutex, MutexGuard};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};
use log::info;
use vulkan_playground::InitOption;
use crate::{GpuFence};
use crate::logic::LogicItems;

pub struct TimingItems {
    pub show_frame_times: bool,
    pub frame_component_durations: FrameComponentDurations,

    frame_render_end: Arc<Mutex<Option<GpuFence>>>,
    async_logic_prod: InitOption<Sender<()>>,
    render_gpu_start: Arc<Mutex<Instant>>,
    async_cpu_start: Arc<Mutex<Instant>>,

    frame_start_moments: VecDeque<Instant>,
    min_frame_duration: Duration,
    main_thread_cons: InitOption<Receiver<(AsynchronousTask, Duration)>>,
}

pub struct FrameComponentDurations {
    pub base_logic_duration: Option<Duration>,
    pub async_logic_duration: Option<Duration>,
    pub ui_duration: Option<Duration>,
    pub render_cpu_duration: Option<Duration>,
    pub render_gpu_duration: Option<Duration>,
    pub gpu_prep_duration: Option<Duration>,
}

enum AsynchronousTask {
    CpuLogic,
    GpuRender,
}

impl TimingItems {

    pub fn get_frame_render_end_mutex(&'_ self) -> MutexGuard<'_, Option<GpuFence>> {
        self.frame_render_end.lock().unwrap()
    }

    pub fn get_async_logic_prod(&mut self) -> &mut Sender<()> {
        self.async_logic_prod.get_mut()
    }

    pub fn get_render_gpu_start_mutex(&'_ self) -> MutexGuard<'_, Instant> {
        self.render_gpu_start.lock().unwrap()
    }

    pub fn get_async_cpu_start_mutex(&'_ self) -> MutexGuard<'_, Instant> {
        self.async_cpu_start.lock().unwrap()
    }

    pub fn new() -> Self {
        let now = Instant::now();

        let min_frame_duration = Duration::from_secs_f32(1.0 / 60.0);
        let mut frame_start_moments: VecDeque<Instant> = VecDeque::new();
        frame_start_moments.push_back(now - min_frame_duration);
        frame_start_moments.push_back(now);

        let mut timing_items = TimingItems {
            frame_component_durations: FrameComponentDurations::empty(),
            frame_render_end: Arc::new(Mutex::new(None)),
            render_gpu_start: Arc::new(Mutex::new(now)),
            async_cpu_start: Arc::new(Mutex::new(now)),
            frame_start_moments,
            min_frame_duration,
            show_frame_times: true,
            async_logic_prod: InitOption::none(),
            main_thread_cons: InitOption::none(),
        };

        timing_items.start_async_processes();

        timing_items
    }

    pub fn get_frame_duration(&self) -> f32 {
        if self.frame_start_moments.len() != 2 {
            panic!("Not enough frame moments in queue");
        }
        let back = *self.frame_start_moments.back().unwrap();
        let front = *self.frame_start_moments.front().unwrap();
        (back - front).as_secs_f32()
    }

    pub fn new_frame_start(&mut self,
                           logic_items: &LogicItems,
    ) -> bool {
        let frame_start_moments = &mut self.frame_start_moments;
        let frame_component_durations = &mut self.frame_component_durations;
        let now = Instant::now();
        let duration_since_last_start = now.duration_since(*frame_start_moments.back().unwrap());

        for message in self.main_thread_cons.try_iter() {
            match message {
                (AsynchronousTask::CpuLogic, duration) => {
                    frame_component_durations.async_logic_duration = Some(duration);
                }
                (AsynchronousTask::GpuRender, duration) => {
                    frame_component_durations.render_gpu_duration = Some(duration);
                }
            }
        }

        let frame_duration_passed = duration_since_last_start > self.min_frame_duration;
        let gpu_prep_duration = frame_component_durations.gpu_prep_duration;
        let async_logic_duration = frame_component_durations.async_logic_duration;
        let render_gpu_duration = frame_component_durations.render_gpu_duration;

        let new_frame_start =
            frame_duration_passed
            && (gpu_prep_duration.is_none()
                || (async_logic_duration.is_some() && render_gpu_duration.is_some()));

        if new_frame_start {
            frame_start_moments.push_back(now);
            frame_start_moments.pop_front();

            if self.show_frame_times {
                info!("Frame {:5} | {}", logic_items.get_frame_id(), self.frame_component_durations);
            }

            self.frame_component_durations = FrameComponentDurations::empty();
        }

        new_frame_start
    }

    fn start_async_processes(&mut self) {
        let (async_logic_prod, async_logic_cons) = channel::<()>();
        let (done_checker_prod, done_checker_cons) = channel::<()>();
        let (main_thread_prod, main_thread_cons) = channel::<(AsynchronousTask, Duration)>();

        thread::spawn(move || {
            loop {
                async_logic_cons.recv().unwrap();

                // expensive logic placeholder
                thread::sleep(Duration::from_millis(4));

                done_checker_prod.send(()).unwrap();
            }
        });

        {
            let async_cpu_start = self.async_cpu_start.clone();
            let render_gpu_start = self.render_gpu_start.clone();
            let frame_render_end = self.frame_render_end.clone();

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

        self.async_logic_prod = InitOption::some(async_logic_prod);
        self.main_thread_cons = InitOption::some(main_thread_cons);
    }
}

impl FrameComponentDurations {

    pub fn empty() -> Self {
        FrameComponentDurations {
            base_logic_duration: None,
            async_logic_duration: None,
            ui_duration: None,
            render_cpu_duration: None,
            render_gpu_duration: None,
            gpu_prep_duration: None,
        }
    }

    fn display_duration(duration: Option<Duration>) -> String {
        match duration {
            None => {format!("{:>4}", "--")}
            Some(duration) => {format!("{:4.1}", duration.as_secs_f32() * 1000.0)}
        }
    }
}

impl Display for FrameComponentDurations {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "gpu prep: {}, render cpu: {}, render gpu {}, ui: {}, base logic: {}, async logic: {}",
               Self::display_duration(self.gpu_prep_duration),
               Self::display_duration(self.render_cpu_duration),
               Self::display_duration(self.render_gpu_duration),
               Self::display_duration(self.ui_duration),
               Self::display_duration(self.base_logic_duration),
               Self::display_duration(self.async_logic_duration),
        )
    }
}
