use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    FadeIn,
    Record,
    FadeOut,
    Idle,
}

pub struct Condenser {
    pub fs: usize,
    th_lin: f32,
    dry_wet: f32,
    loop_mode: bool,

    warmup_frames: usize,
    processed_frames: usize,

    max_frames: usize,
    buf: Vec<f32>,
    write_ptr: usize,
    read_ptr: usize,
    recorded_frames: usize,

    state: State,
    fade_len: usize,
    fade_curve: Vec<f32>,
    fade_pos: usize,

    rel_coef: f32,
    env: f32,
}

impl Condenser {
    pub fn new(
        fs: usize,
        threshold_db: f32,
        dry_wet: f32,
        fade_ms: f32,
        rel_ms: f32,
        max_seconds: usize,
        warmup_sec: f32,
        loop_mode: bool,
    ) -> Self {
        let th_lin = 10f32.powf(threshold_db / 20.0);
        let dry_wet = dry_wet.clamp(0.0, 1.0);
        let warmup_frames = (warmup_sec * fs as f32) as usize;
        let max_frames = fs * max_seconds;
        let fade_len = ((fade_ms * 1e-3 * fs as f32) as usize).max(1);
        let mut fade_curve = Vec::with_capacity(fade_len);
        for t in 0..fade_len {
            fade_curve.push(0.5 - 0.5 * (2.0 * PI * t as f32 / (fade_len as f32 - 1.0)).cos());
        }
        let rel_coef = (-1.0 / (rel_ms * 1e-3 * fs as f32)).exp();

        Self {
            fs,
            th_lin,
            dry_wet,
            loop_mode,
            warmup_frames,
            processed_frames: 0,
            max_frames,
            buf: vec![0.0; max_frames],
            write_ptr: 0,
            read_ptr: 0,
            recorded_frames: 0,
            state: State::Idle,
            fade_len,
            fade_curve,
            fade_pos: 0,
            rel_coef,
            env: 0.0,
        }
    }

    fn ring_write(&mut self, data: &[f32]) {
        if self.loop_mode { return; }
        let n = data.len();
        let end = self.write_ptr + n;
        if end <= self.max_frames {
            self.buf[self.write_ptr..end].copy_from_slice(data);
        } else {
            let first = self.max_frames - self.write_ptr;
            self.buf[self.write_ptr..].copy_from_slice(&data[..first]);
            self.buf[..n-first].copy_from_slice(&data[first..]);
        }
        self.write_ptr = end % self.max_frames;
        self.recorded_frames = self.recorded_frames.max(end).min(self.max_frames);
    }

    fn ring_read(&mut self, n: usize) -> Vec<f32> {
        if self.recorded_frames == 0 {
            return vec![0.0; n];
        }
        let loop_len = self.recorded_frames;
        let end = self.read_ptr + n;
        let out;
        if end <= loop_len {
            out = self.buf[self.read_ptr..end].to_vec();
        } else {
            let first = loop_len - self.read_ptr;
            out = [&self.buf[self.read_ptr..loop_len], &self.buf[..n-first]].concat();
        }
        self.read_ptr = (self.read_ptr + n) % loop_len;
        out
    }

    pub fn process_inplace(&mut self, block: &mut [f32]) {
        let n_total = block.len();

        if self.loop_mode {
            let wet = self.ring_read(n_total);
            for (m, w) in block.iter_mut().zip(wet.iter()) {
                *m = (1.0 - self.dry_wet) * *m + self.dry_wet * *w;
            }
            return;
        }

        if self.processed_frames < self.warmup_frames {
            self.processed_frames += n_total;
            let wet = self.ring_read(n_total);
            for (m, w) in block.iter_mut().zip(wet.iter()) {
                *m = (1.0 - self.dry_wet) * *m + self.dry_wet * *w;
            }
            return;
        }

        let mut idx = 0;
        while idx < n_total {
            let remain = n_total - idx;
            let seg = &block[idx..idx + remain];

            let peak = seg.iter().fold(0.0f32, |a,&b| a.max(b.abs()));
            self.env = if peak > self.env { peak } else { self.env * self.rel_coef.powi(remain as i32) };

            match self.state {
                State::Idle => {
                    if self.env > self.th_lin {
                        self.state = State::FadeIn;
                        self.fade_pos = 0;
                    } else {
                        break;
                    }
                }
                _ => {}
            }

            match self.state {
                State::FadeIn => {
                    let span = remain.min(self.fade_len - self.fade_pos);
                    let fade_slice = &self.fade_curve[self.fade_pos..self.fade_pos + span];
                    let data: Vec<f32> = seg[..span]
                        .iter()
                        .zip(fade_slice)
                        .map(|(s, f)| s * f)
                        .collect();
                    self.ring_write(&data);
                    idx += span;
                    self.fade_pos += span;
                    if self.fade_pos >= self.fade_len {
                        self.state = State::Record;
                    }
                    continue;
                }
                State::Record => {
                    if self.env > self.th_lin {
                        self.ring_write(seg);
                        idx = n_total;
                    } else {
                        self.state = State::FadeOut;
                        self.fade_pos = 0;
                    }
                    continue;
                }
                State::FadeOut => {
                    let span = remain.min(self.fade_len - self.fade_pos);
                    let fade_slice: Vec<f32> = self.fade_curve
                        [self.fade_len - span - self.fade_pos..self.fade_len - self.fade_pos]
                        .iter()
                        .rev()
                        .cloned()
                        .collect();
                    let data: Vec<f32> = seg[..span]
                        .iter()
                        .zip(fade_slice)
                        .map(|(s, f)| s * f)
                        .collect();
                    self.ring_write(&data);
                    idx += span;
                    self.fade_pos += span;
                    if self.fade_pos >= self.fade_len {
                        self.state = State::Idle;
                    }
                    continue;
                }
                State::Idle => {}
            }
        }
        self.processed_frames += n_total;
        let wet = self.ring_read(n_total);
        for (m, w) in block.iter_mut().zip(wet.iter()) {
            *m = (1.0 - self.dry_wet) * *m + self.dry_wet * *w;
        }
    }

    pub fn get_recorded(&self) -> Vec<f32> {
        self.buf[..self.recorded_frames].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_write_read() {
        let mut c = Condenser::new(10, -10.0, 1.0, 1.0, 1.0, 2, 0.0, false);
        c.ring_write(&[1.0,2.0,3.0]);
        assert_eq!(c.ring_read(3), vec![1.0,2.0,3.0]);
    }

    #[test]
    fn loop_mode_playback() {
        let mut c = Condenser::new(10, -10.0, 1.0, 1.0, 1.0, 2, 0.0, true);
        c.buf[..3].copy_from_slice(&[1.0,2.0,3.0]);
        c.recorded_frames = 3;
        let mut data = [0.0,0.0,0.0,0.0];
        c.process_inplace(&mut data);
        assert_eq!(data.to_vec(), vec![1.0,2.0,3.0,1.0]);
    }

    #[test]
    fn ring_wraparound() {
        let mut c = Condenser::new(4, -10.0, 1.0, 1.0, 1.0, 1, 0.0, false);
        c.ring_write(&[1.0,2.0,3.0,4.0]);
        assert_eq!(c.ring_read(2), vec![1.0,2.0]);
        c.ring_write(&[5.0,6.0]);
        assert_eq!(c.ring_read(4), vec![3.0,4.0,5.0,6.0]);
    }

    #[test]
    fn record_and_fade() {
        let mut c = Condenser::new(10, -60.0, 1.0, 300.0, 10.0, 10, 0.0, false);
        let mut blk1 = [1.0; 3];
        c.process_inplace(&mut blk1);
        assert_eq!(c.get_recorded(), vec![0.0, 1.0, 0.0]);
        assert_eq!(c.state, State::Record);

        let mut blk2 = [0.0; 3];
        c.process_inplace(&mut blk2);
        assert_eq!(c.get_recorded(), vec![0.0, 1.0, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(c.state, State::Idle);
    }

    #[test]
    fn dry_wet_mix() {
        let mut c = Condenser::new(10, -10.0, 0.5, 1.0, 1.0, 2, 0.0, true);
        c.buf[..3].copy_from_slice(&[1.0,1.0,1.0]);
        c.recorded_frames = 3;
        let mut data = [0.0,0.0,0.0];
        c.process_inplace(&mut data);
        assert_eq!(data.to_vec(), vec![0.5,0.5,0.5]);
    }

    #[test]
    fn warmup_skip() {
        let mut c = Condenser::new(10, -60.0, 1.0, 3.0, 1.0, 10, 0.2, false);
        let mut pre = [1.0,1.0];
        c.process_inplace(&mut pre);
        assert_eq!(c.recorded_frames, 0);

        let mut post = [1.0,1.0];
        c.process_inplace(&mut post);
        assert!(c.recorded_frames > 0);
    }
}

