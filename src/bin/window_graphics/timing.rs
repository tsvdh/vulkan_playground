use std::collections::VecDeque;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use crate::{App, AsynchronousTask, FrameComponentDurations, TimingItems};

impl TimingItems {

    pub fn new() -> Self {
        let min_frame_duration = Duration::from_secs_f32(1.0 / 60.0);

        let mut frame_start_moments: VecDeque<Instant> = VecDeque::new();
        let now = Instant::now();
        frame_start_moments.push_back(now - min_frame_duration);
        frame_start_moments.push_back(now);

        TimingItems {
            frame_component_durations: FrameComponentDurations::empty(),
            frame_render_end: Arc::new(Mutex::new(None)),
            render_gpu_start: Arc::new(Mutex::new(now)),
            async_cpu_start: Arc::new(Mutex::new(now)),
            frame_start_moments,
            min_frame_duration,
        }
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

impl App {
    pub fn get_frame_duration(&self) -> f32 {
        if self.timing_items.frame_start_moments.len() != 2 {
            panic!("Not enough frame moments in queue");
        }
        let back = *self.timing_items.frame_start_moments.back().unwrap();
        let front = *self.timing_items.frame_start_moments.front().unwrap();
        (back - front).as_secs_f32()
    }

    pub fn new_frame_start(&mut self) -> bool {
        let frame_start_moments = &mut self.timing_items.frame_start_moments;
        let now = Instant::now();
        let duration_since_last_start = now.duration_since(*frame_start_moments.back().unwrap());

        for message in self.async_management.main_thread_cons.try_iter() {
            match message {
                (AsynchronousTask::CpuLogic, duration) => {
                    self.timing_items.frame_component_durations.async_logic_duration = Some(duration);
                }
                (AsynchronousTask::GpuRender, duration) => {
                    self.timing_items.frame_component_durations.render_gpu_duration = Some(duration);
                }
            }
        }

        if duration_since_last_start > self.timing_items.min_frame_duration
            && self.timing_items.frame_component_durations.gpu_prep_duration.is_none()
            || (self.timing_items.frame_component_durations.async_logic_duration.is_some()
                && self.timing_items.frame_component_durations.render_gpu_duration.is_some())
        {
            frame_start_moments.push_back(now);
            frame_start_moments.pop_front();
            return true;
        }

        false
    }
}