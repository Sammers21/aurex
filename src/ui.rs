use std::{
    env,
    io::{self, IsTerminal, Write},
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use aurex::{BuildEvent, BuildEventDetail, BuildReporter, BuildStage, JavaInfo};
use crossterm::{
    cursor::{Hide, MoveToColumn, MoveUp, Show},
    queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{self, Clear, ClearType},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildUiStyle {
    Full,
    Quiet,
}

pub struct CliBuildReporter {
    inner: ReporterInner,
}

enum ReporterInner {
    Animated(AnimatedBuildReporter),
    Plain(PlainBuildReporter),
}

impl CliBuildReporter {
    pub fn new(command: &'static str, artifact: PathBuf, style: BuildUiStyle) -> Self {
        let inner = if animation_enabled(style) {
            ReporterInner::Animated(AnimatedBuildReporter::new(command, artifact, style))
        } else {
            ReporterInner::Plain(PlainBuildReporter::new(command, style))
        };
        Self { inner }
    }

    pub fn finish_success(&mut self) {
        match &mut self.inner {
            ReporterInner::Animated(reporter) => reporter.finish_success(),
            ReporterInner::Plain(reporter) => reporter.finish_success(),
        }
    }

    pub fn finish_error(&mut self, error: &str) {
        match &mut self.inner {
            ReporterInner::Animated(reporter) => reporter.finish_error(error),
            ReporterInner::Plain(reporter) => reporter.finish_error(error),
        }
    }
}

impl BuildReporter for CliBuildReporter {
    fn report(&mut self, event: BuildEvent) {
        match &mut self.inner {
            ReporterInner::Animated(reporter) => reporter.report(event),
            ReporterInner::Plain(reporter) => reporter.report(event),
        }
    }

    fn tick(&mut self) {
        if let ReporterInner::Animated(reporter) = &mut self.inner {
            reporter.tick();
        }
    }
}

struct PlainBuildReporter {
    command: &'static str,
    style: BuildUiStyle,
    printed_header: bool,
    finished: bool,
}

impl PlainBuildReporter {
    fn new(command: &'static str, style: BuildUiStyle) -> Self {
        Self {
            command,
            style,
            printed_header: false,
            finished: false,
        }
    }

    fn finish_success(&mut self) {
        self.finished = true;
    }

    fn finish_error(&mut self, error: &str) {
        self.print_header();
        eprintln!("error: {error}");
        self.finished = true;
    }

    fn print_header(&mut self) {
        if !self.printed_header {
            eprintln!("{}", self.command);
            self.printed_header = true;
        }
    }

    fn print_output(&mut self, text: &str) {
        self.print_header();
        eprint!("{text}");
        if !text.ends_with('\n') {
            eprintln!();
        }
    }
}

impl BuildReporter for PlainBuildReporter {
    fn report(&mut self, event: BuildEvent) {
        self.print_header();
        match event {
            BuildEvent::Started(stage) if self.style == BuildUiStyle::Full => {
                eprintln!("{} ...", stage.as_str());
            }
            BuildEvent::Started(_) => {}
            BuildEvent::Finished(stage, detail) => {
                eprintln!("{} ok ({})", stage.as_str(), plain_detail(&detail));
            }
            BuildEvent::Output { text, .. } => {
                self.print_output(&text);
            }
            BuildEvent::Done { jar_path } => {
                eprintln!("done {}", jar_path.display());
                self.finished = true;
            }
        }
    }
}

struct AnimatedBuildReporter {
    command: &'static str,
    artifact: PathBuf,
    style: BuildUiStyle,
    stages: Vec<StageRow>,
    progress: f32,
    rendered_height: usize,
    started_at: Instant,
    outputs: Vec<String>,
    finished: bool,
}

#[derive(Clone)]
struct StageRow {
    stage: BuildStage,
    state: StageState,
    detail: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StageState {
    Pending,
    Active,
    Done,
}

impl AnimatedBuildReporter {
    fn new(command: &'static str, artifact: PathBuf, style: BuildUiStyle) -> Self {
        Self {
            command,
            artifact,
            style,
            stages: vec![
                StageRow::new(BuildStage::Resolve, "Maven dependencies"),
                StageRow::new(BuildStage::Compile, "javac source set"),
                StageRow::new(BuildStage::Resources, "configured resource dirs"),
                StageRow::new(BuildStage::Package, "jar archive"),
            ],
            progress: 0.0,
            rendered_height: 0,
            started_at: Instant::now(),
            outputs: Vec::new(),
            finished: false,
        }
    }

    fn finish_success(&mut self) {
        if !self.finished {
            self.finish_frame();
            self.flush_outputs();
        }
    }

    fn finish_error(&mut self, error: &str) {
        if !self.finished && self.rendered_height > 0 {
            self.finish_frame();
        }
        self.flush_outputs();
        render_error(error);
        self.finished = true;
    }

    fn set_active(&mut self, stage: BuildStage) {
        for row in &mut self.stages {
            if row.stage == stage && row.state != StageState::Done {
                row.state = StageState::Active;
            }
        }
    }

    fn set_done(&mut self, stage: BuildStage, detail: BuildEventDetail) {
        for row in &mut self.stages {
            if row.stage == stage {
                row.state = StageState::Done;
                row.detail = animated_detail(&detail);
            }
        }
        if let BuildEventDetail::Artifact(path) = detail {
            self.artifact = path;
        }
    }

    fn animate_to(&mut self, target: f32, final_frame: bool) {
        let frames = match self.style {
            BuildUiStyle::Full if final_frame => 6,
            BuildUiStyle::Full => 4,
            BuildUiStyle::Quiet => 1,
        };
        let delay = match self.style {
            BuildUiStyle::Full => Duration::from_millis(24),
            BuildUiStyle::Quiet => Duration::from_millis(0),
        };
        let start = self.progress;
        let _cursor = CursorGuard::hide();
        for frame in 1..=frames {
            let ratio = frame as f32 / frames as f32;
            let progress = start + (target - start) * ratio;
            let is_final_frame = final_frame && frame == frames;
            let _ = self.draw(progress, is_final_frame);
            if delay > Duration::ZERO && frame < frames {
                thread::sleep(delay);
            }
        }
        self.progress = target;
    }

    fn tick(&mut self) {
        if self.finished || self.rendered_height == 0 {
            return;
        }

        let _cursor = CursorGuard::hide();
        let _ = self.draw(self.progress, false);
    }

    fn draw(&mut self, progress: f32, final_frame: bool) -> io::Result<()> {
        let width = terminal::size()
            .map(|(width, _)| width as usize)
            .unwrap_or(100)
            .saturating_sub(1)
            .clamp(1, 100);
        let lines = self.render(progress, final_frame);
        draw_lines(&mut io::stderr(), lines, width, &mut self.rendered_height)
    }

    fn render(&self, progress: f32, final_frame: bool) -> Vec<StyledLine> {
        let animation_progress = self.animation_progress(progress, final_frame);
        let mut lines = Vec::new();
        lines.push(parts(vec![
            seg("$ ", dim(), false),
            seg(self.command, white(), true),
        ]));
        lines.extend(coin_toss_lines(animation_progress, final_frame));
        lines.push(blank());
        for row in &self.stages {
            lines.push(stage_line(row));
        }
        lines.push(summary_line(
            self.command,
            &self.artifact,
            self.started_at.elapsed(),
            final_frame,
            progress,
            animation_progress,
        ));
        lines
    }

    fn animation_progress(&self, progress: f32, final_frame: bool) -> f32 {
        if final_frame {
            progress
        } else {
            progress + self.started_at.elapsed().as_secs_f32() * 0.6
        }
    }

    fn finish_frame(&mut self) {
        let mut stderr = io::stderr();
        let _ = queue!(stderr, Print("\r\n"));
        let _ = stderr.flush();
        self.rendered_height = 0;
        self.finished = true;
    }

    fn flush_outputs(&mut self) {
        if self.outputs.is_empty() {
            return;
        }

        let mut stderr = io::stderr();
        for output in self.outputs.drain(..) {
            let _ = queue!(stderr, Print(&output));
            if !output.ends_with('\n') {
                let _ = queue!(stderr, Print("\r\n"));
            }
        }
        let _ = stderr.flush();
    }
}

impl BuildReporter for AnimatedBuildReporter {
    fn report(&mut self, event: BuildEvent) {
        match event {
            BuildEvent::Started(stage) => {
                self.set_active(stage);
                self.animate_to(stage_start_progress(stage), false);
            }
            BuildEvent::Finished(stage, detail) => {
                self.set_done(stage, detail);
                self.animate_to(stage_end_progress(stage), false);
            }
            BuildEvent::Output { text, .. } => {
                self.outputs.push(text);
            }
            BuildEvent::Done { jar_path } => {
                self.artifact = jar_path;
                self.animate_to(1.0, true);
                self.finish_frame();
                self.flush_outputs();
            }
        }
    }
}

impl StageRow {
    fn new(stage: BuildStage, detail: &'static str) -> Self {
        Self {
            stage,
            state: StageState::Pending,
            detail: detail.to_string(),
        }
    }
}

struct CursorGuard;

impl CursorGuard {
    fn hide() -> Self {
        let mut stderr = io::stderr();
        let _ = queue!(stderr, Hide);
        let _ = stderr.flush();
        Self
    }
}

impl Drop for CursorGuard {
    fn drop(&mut self) {
        let mut stderr = io::stderr();
        let _ = queue!(stderr, Show);
        let _ = stderr.flush();
    }
}

#[derive(Clone)]
struct Segment {
    text: String,
    fg: Option<Color>,
    bold: bool,
}

type StyledLine = Vec<Segment>;

fn draw_lines(
    stderr: &mut io::Stderr,
    lines: Vec<StyledLine>,
    width: usize,
    rendered_height: &mut usize,
) -> io::Result<()> {
    let height = lines.len();
    if *rendered_height > 0 {
        queue!(
            stderr,
            MoveUp(rendered_height.saturating_sub(1).min(u16::MAX as usize) as u16)
        )?;
    } else if height > 1 {
        for _ in 1..height {
            queue!(stderr, Print("\r\n"))?;
        }
        queue!(stderr, MoveUp((height - 1).min(u16::MAX as usize) as u16))?;
    }
    queue!(stderr, MoveToColumn(0))?;
    let reserved_height = height.max(*rendered_height);
    for (index, line) in lines.into_iter().enumerate() {
        write_line(stderr, line, width, index + 1 < reserved_height)?;
    }
    for index in height..*rendered_height {
        write_line(stderr, blank(), width, index + 1 < reserved_height)?;
    }
    stderr.flush()?;
    *rendered_height = reserved_height;
    Ok(())
}

fn write_line(
    stderr: &mut io::Stderr,
    line: StyledLine,
    width: usize,
    newline_after: bool,
) -> io::Result<()> {
    let mut remaining = width;
    queue!(stderr, MoveToColumn(0), Clear(ClearType::UntilNewLine))?;
    for mut segment in line {
        if remaining == 0 {
            break;
        }
        let len = segment.text.chars().count();
        if len > remaining {
            segment.text = segment.text.chars().take(remaining).collect();
        }
        let printed_len = segment.text.chars().count();
        if let Some(color) = segment.fg {
            queue!(stderr, SetForegroundColor(color))?;
        }
        if segment.bold {
            queue!(stderr, SetAttribute(Attribute::Bold))?;
        }
        queue!(
            stderr,
            Print(&segment.text),
            ResetColor,
            SetAttribute(Attribute::Reset)
        )?;
        remaining = remaining.saturating_sub(printed_len);
    }
    queue!(stderr, Clear(ClearType::UntilNewLine))?;
    if newline_after {
        queue!(stderr, Print("\r\n"))?;
    }
    Ok(())
}

fn coin_toss_lines(progress: f32, final_frame: bool) -> Vec<StyledLine> {
    let frame = coin_toss_frame(progress, final_frame);
    frame
        .rows
        .into_iter()
        .enumerate()
        .map(|(row_index, row)| {
            if row_index == frame.coin_row {
                coin_line(row, frame.coin_col)
            } else {
                parts(vec![seg(row, dim(), false)])
            }
        })
        .collect()
}

struct TossFrame {
    rows: Vec<String>,
    coin_row: usize,
    coin_col: usize,
}

fn coin_toss_frame(progress: f32, final_frame: bool) -> TossFrame {
    let width = 20;
    let mut rows = vec![vec![' '; width]; 11];
    let cycle = (progress * 7.0).fract();
    let hand = hand_frame(cycle, final_frame);
    draw_hand(&mut rows, &hand);

    let (coin_row, coin_col, coin) = if final_frame {
        (hand.coin_row, hand.coin_col, '●')
    } else if !(0.24..0.82).contains(&cycle) {
        (hand.coin_row, hand.coin_col, '●')
    } else {
        let air = (cycle - 0.24) / 0.58;
        let lift = (std::f32::consts::PI * air).sin();
        let coin_row = (4.0 - lift * 4.0).round().clamp(0.0, 4.0) as usize;
        (coin_row, hand.coin_col, coin_spin(progress))
    };

    if !final_frame && (0.24..0.82).contains(&cycle) && coin_row < hand.coin_row {
        let air = (cycle - 0.24) / 0.58;
        let trail_row = if air < 0.5 {
            (coin_row + 1).min(hand.coin_row - 1)
        } else {
            coin_row.saturating_sub(1)
        };
        if trail_row != coin_row {
            put_char(&mut rows, trail_row, coin_col, '·');
        }
    }
    rows[coin_row][coin_col] = coin;

    TossFrame {
        rows: rows
            .into_iter()
            .map(|row| row.into_iter().collect())
            .collect(),
        coin_row,
        coin_col,
    }
}

#[derive(Clone, Copy)]
enum HandPose {
    Settle,
    Load,
    Snap,
    Open,
    Catch,
}

struct HandFrame {
    top: usize,
    coin_row: usize,
    coin_col: usize,
    art: [&'static str; 4],
}

fn hand_frame(cycle: f32, final_frame: bool) -> HandFrame {
    let (pose, top) = if final_frame {
        (HandPose::Settle, 6)
    } else if cycle < 0.08 {
        (HandPose::Settle, 6)
    } else if cycle < 0.16 {
        (HandPose::Load, 7)
    } else if cycle < 0.24 {
        (HandPose::Snap, 5)
    } else if cycle < 0.72 {
        (HandPose::Open, 6)
    } else if cycle < 0.90 {
        (HandPose::Catch, 5)
    } else {
        (HandPose::Settle, 6)
    };

    HandFrame {
        top,
        coin_row: top.saturating_sub(1),
        coin_col: 10,
        art: hand_art(pose),
    }
}

fn hand_art(pose: HandPose) -> [&'static str; 4] {
    match pose {
        HandPose::Settle => [
            "     _________  ",
            " ___/_________)",
            "/___  _______/ ",
            "    \\_/        ",
        ],
        HandPose::Load => [
            "     __\\_/____ ",
            " ___/_________)",
            "/___  _______/ ",
            "   _\\_/        ",
        ],
        HandPose::Snap => [
            "      ___/____ ",
            "  ___/________)",
            " _/___  _____/ ",
            "     \\_/       ",
        ],
        HandPose::Open => [
            "      ________ ",
            " ___/_________)",
            "/___  _______/ ",
            "    \\_/        ",
        ],
        HandPose::Catch => [
            "     __\\_/____ ",
            " ___/_________)",
            "/___  _______/ ",
            "    \\_/        ",
        ],
    }
}

fn draw_hand(rows: &mut [Vec<char>], hand: &HandFrame) {
    for (offset, line) in hand.art.iter().enumerate() {
        if let Some(row) = rows.get_mut(hand.top + offset) {
            put_text(row, 1, line);
        }
    }
}

fn coin_spin(progress: f32) -> char {
    const SPIN: [char; 8] = ['◉', '◐', '╱', '◑', '◉', '◒', '╲', '◓'];
    SPIN[((progress * 56.0).floor() as usize) % SPIN.len()]
}

fn coin_line(row: String, coin_col: usize) -> StyledLine {
    let before: String = row.chars().take(coin_col).collect();
    let coin: String = row
        .chars()
        .nth(coin_col)
        .map(|ch| ch.to_string())
        .unwrap_or_default();
    let after: String = row.chars().skip(coin_col + 1).collect();
    parts(vec![
        seg(before, dim(), false),
        seg(coin, gold(), true),
        seg(after, dim(), false),
    ])
}

fn put_text(row: &mut [char], start: usize, text: &str) {
    for (offset, ch) in text.chars().enumerate() {
        if let Some(slot) = row.get_mut(start + offset) {
            *slot = ch;
        }
    }
}

fn put_char(rows: &mut [Vec<char>], row: usize, col: usize, ch: char) {
    if let Some(slot) = rows.get_mut(row).and_then(|line| line.get_mut(col)) {
        *slot = ch;
    }
}

fn stage_line(row: &StageRow) -> StyledLine {
    let (marker, marker_color, bold) = match row.state {
        StageState::Pending => ("·", dim(), false),
        StageState::Active => ("●", gold(), true),
        StageState::Done => ("✓", success(), true),
    };
    let name_color = if row.state == StageState::Pending {
        dim()
    } else {
        white()
    };
    let detail_color = if row.state == StageState::Pending {
        dark()
    } else {
        dim()
    };
    parts(vec![
        seg(format!("{marker} "), marker_color, bold),
        seg(format!("{:<9}", row.stage.as_str()), name_color, false),
        seg(" ", None, false),
        seg(&row.detail, detail_color, false),
    ])
}

fn summary_line(
    command: &str,
    artifact: &PathBuf,
    elapsed: Duration,
    final_frame: bool,
    progress: f32,
    animation_progress: f32,
) -> StyledLine {
    if final_frame {
        parts(vec![
            seg("✓ ", success(), true),
            seg("built ", success(), true),
            seg(artifact.display().to_string(), gold(), true),
            seg(format!(" in {}ms", elapsed.as_millis()), dim(), false),
        ])
    } else {
        parts(vec![
            seg("  active ", dim(), false),
            seg(active_label(progress), gold(), true),
            seg("  ", None, false),
            seg(coin_toss_label(animation_progress), dim(), false),
            seg("  ", None, false),
            seg(command, dark(), false),
        ])
    }
}

fn stage_start_progress(stage: BuildStage) -> f32 {
    stage_index(stage) as f32 / 4.0 * 0.88
}

fn stage_end_progress(stage: BuildStage) -> f32 {
    (stage_index(stage) as f32 + 1.0) / 4.0 * 0.88
}

fn stage_index(stage: BuildStage) -> usize {
    match stage {
        BuildStage::Resolve => 0,
        BuildStage::Compile => 1,
        BuildStage::Resources => 2,
        BuildStage::Package => 3,
    }
}

fn active_label(progress: f32) -> &'static str {
    if progress < stage_end_progress(BuildStage::Resolve) {
        "resolve"
    } else if progress < stage_end_progress(BuildStage::Compile) {
        "compile"
    } else if progress < stage_end_progress(BuildStage::Resources) {
        "resources"
    } else {
        "package"
    }
}

fn coin_toss_label(progress: f32) -> &'static str {
    let cycle = (progress * 7.0).fract();
    if cycle < 0.10 {
        "palm loads"
    } else if cycle < 0.24 {
        "palm flicks"
    } else if cycle < 0.45 {
        "coin rises"
    } else if cycle < 0.60 {
        "coin turns"
    } else if cycle < 0.82 {
        "coin drops"
    } else if cycle < 0.90 {
        "palm catches"
    } else {
        "palm settles"
    }
}

fn animated_detail(detail: &BuildEventDetail) -> String {
    match detail {
        BuildEventDetail::None => "complete".to_string(),
        BuildEventDetail::Artifacts(count) => match count {
            0 => "no external artifacts".to_string(),
            1 => "1 artifact available".to_string(),
            count => format!("{count} artifacts available"),
        },
        BuildEventDetail::Sources(count) => match count {
            1 => "1 source compiled".to_string(),
            count => format!("{count} sources compiled"),
        },
        BuildEventDetail::Resources(count) => match count {
            0 => "no resources configured".to_string(),
            1 => "1 resource copied".to_string(),
            count => format!("{count} resources copied"),
        },
        BuildEventDetail::Artifact(path) => format!("wrote {}", path.display()),
    }
}

fn plain_detail(detail: &BuildEventDetail) -> String {
    match detail {
        BuildEventDetail::None => "complete".to_string(),
        BuildEventDetail::Artifacts(count) => format!("{count} artifacts"),
        BuildEventDetail::Sources(count) => format!("{count} sources"),
        BuildEventDetail::Resources(count) => format!("{count} resources"),
        BuildEventDetail::Artifact(path) => path.display().to_string(),
    }
}

pub fn render_init_success(root: &str) {
    if color_enabled_stderr() {
        let mut stderr = io::stderr();
        let _ = queue!(
            stderr,
            SetForegroundColor(success()),
            SetAttribute(Attribute::Bold),
            Print("✓ "),
            Print("created "),
            ResetColor,
            SetAttribute(Attribute::Reset),
            SetForegroundColor(gold()),
            SetAttribute(Attribute::Bold),
            Print("Aurex project"),
            ResetColor,
            SetAttribute(Attribute::Reset),
            Print(" at "),
            SetForegroundColor(dim()),
            Print(root),
            ResetColor,
            Print("\r\n")
        );
        let _ = stderr.flush();
    } else {
        eprintln!("created Aurex project at {root}");
    }
}

pub fn render_error(error: &str) {
    if color_enabled_stderr() {
        let mut stderr = io::stderr();
        let _ = queue!(
            stderr,
            SetForegroundColor(error_color()),
            SetAttribute(Attribute::Bold),
            Print("error"),
            ResetColor,
            SetAttribute(Attribute::Reset),
            Print(": "),
            Print(error),
            Print("\r\n")
        );
        let _ = stderr.flush();
    } else {
        eprintln!("error: {error}");
    }
}

pub fn render_java_info(info: &JavaInfo) {
    if color_enabled_stdout() {
        let mut stdout = io::stdout();
        let _ = queue!(
            stdout,
            SetForegroundColor(gold()),
            SetAttribute(Attribute::Bold),
            Print("java"),
            ResetColor,
            SetAttribute(Attribute::Reset),
            Print(": "),
            SetForegroundColor(white()),
            Print(info.executable.display()),
            ResetColor,
            Print("\n"),
            SetForegroundColor(dim()),
            Print(&info.version_output),
            ResetColor
        );
        let _ = stdout.flush();
    } else {
        print!(
            "java: {}\n{}",
            info.executable.display(),
            info.version_output
        );
    }
}

fn animation_enabled(_style: BuildUiStyle) -> bool {
    visual_env_enabled() && io::stdout().is_terminal() && io::stderr().is_terminal()
}

fn color_enabled_stdout() -> bool {
    visual_env_enabled() && io::stdout().is_terminal()
}

fn color_enabled_stderr() -> bool {
    visual_env_enabled() && io::stderr().is_terminal()
}

fn visual_env_enabled() -> bool {
    env::var_os("CI").is_none()
        && env::var_os("NO_COLOR").is_none()
        && env::var("TERM").map(|term| term != "dumb").unwrap_or(true)
}

fn parts(parts: Vec<Segment>) -> StyledLine {
    parts
}

fn blank() -> StyledLine {
    vec![seg("", None, false)]
}

fn seg<T: Into<String>>(text: T, fg: impl Into<Option<Color>>, bold: bool) -> Segment {
    Segment {
        text: text.into(),
        fg: fg.into(),
        bold,
    }
}

fn white() -> Color {
    rgb(231, 235, 244)
}

fn dim() -> Color {
    rgb(136, 146, 162)
}

fn dark() -> Color {
    rgb(70, 78, 92)
}

fn gold() -> Color {
    rgb(245, 196, 87)
}

fn success() -> Color {
    rgb(105, 211, 164)
}

fn error_color() -> Color {
    rgb(255, 112, 112)
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb { r, g, b }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn animated_reporter_buffers_output_until_frame_finishes() {
        let mut reporter =
            AnimatedBuildReporter::new("ax run", PathBuf::from("./app.jar"), BuildUiStyle::Quiet);

        reporter.report(BuildEvent::Output {
            stage: BuildStage::Compile,
            text: "warning\n".to_string(),
        });

        assert_eq!(reporter.outputs, vec!["warning\n"]);
    }

    #[test]
    fn animation_progress_advances_without_stage_progress() {
        let mut reporter =
            AnimatedBuildReporter::new("ax run", PathBuf::from("./app.jar"), BuildUiStyle::Quiet);
        reporter.started_at = Instant::now() - Duration::from_millis(100);
        let first = reporter.animation_progress(0.25, false);
        reporter.started_at = Instant::now() - Duration::from_millis(600);
        let second = reporter.animation_progress(0.25, false);

        assert!(second > first);
    }
}
