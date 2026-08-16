//! Askama template structs. Kept separate from the handlers that build them so
//! the list of pages is visible in one place.

use askama::Template;

use crate::handlers::calendar::BlockView;
use crate::handlers::today::{
    DueGroupView, FocusView, JumpBackInItemView, MomentumDay, SpaceProgressRow, UpNextItemView,
};
use crate::models::{Attachment, PlanStep};
use crate::view::{SelectOption, SpaceView, TaskView};

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    pub error: Option<String>,
}

#[derive(Template)]
#[template(path = "signup.html")]
pub struct SignupTemplate {
    pub error: Option<String>,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct DashboardTemplate {
    pub display_name: String,
    pub active_spaces: Vec<SpaceView>,
    pub archived_spaces: Vec<SpaceView>,
    pub color_options: Vec<SelectOption>,
}

#[derive(Template)]
#[template(path = "space_detail.html")]
pub struct SpaceDetailTemplate {
    pub display_name: String,
    pub space: SpaceView,
    pub tasks_html: String,
    pub color_options: Vec<SelectOption>,
    pub priority_options: Vec<SelectOption>,
}

#[derive(Template)]
#[template(path = "partials/task_row.html")]
pub struct TaskRowTemplate {
    pub task: TaskView,
    pub status_options: Vec<SelectOption>,
}

#[derive(Template)]
#[template(path = "task_detail.html")]
pub struct TaskDetailTemplate {
    pub display_name: String,
    pub space_name: String,
    pub space_color: &'static str,
    pub task: TaskView,
    pub steps: Vec<PlanStep>,
    pub attachments: Vec<Attachment>,
    pub max_attachment_mb: i64,
}

#[derive(Template)]
#[template(path = "calendar.html")]
pub struct CalendarTemplate {
    pub display_name: String,
    pub recurring: Vec<BlockView>,
    pub one_off: Vec<BlockView>,
    pub spaces: Vec<(i64, String)>,
    pub color_options: Vec<SelectOption>,
}

// ---------------------------------------------------------------------------
// Today — shell + one template per lazily-loaded card
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "today.html")]
pub struct TodayTemplate {
    pub display_name: String,
    pub order: Vec<String>,
    /// JSON array of `{id,name,color}` for the quick-add "#Space" resolver and
    /// the command palette's client-side fallback list — embedded once here
    /// instead of round-tripping on every keystroke.
    pub spaces_json: String,
}

#[derive(Template)]
#[template(path = "partials/focus.html")]
pub struct FocusTemplate {
    pub greeting: String,
    pub date_str: String,
    pub focus: FocusView,
}

#[derive(Template)]
#[template(path = "partials/up_next.html")]
pub struct UpNextTemplate {
    pub items: Vec<UpNextItemView>,
    pub now_line_index: usize,
}

#[derive(Template)]
#[template(path = "partials/due_soon.html")]
pub struct DueSoonTemplate {
    pub groups: Vec<DueGroupView>,
}

#[derive(Template)]
#[template(path = "partials/momentum.html")]
pub struct MomentumTemplate {
    pub weeks: Vec<Vec<MomentumDay>>,
    pub streak: i64,
    pub completed_this_month: i64,
}

#[derive(Template)]
#[template(path = "partials/jump_back_in.html")]
pub struct JumpBackInTemplate {
    pub items: Vec<JumpBackInItemView>,
}

#[derive(Template)]
#[template(path = "partials/space_progress.html")]
pub struct SpaceProgressTemplate {
    pub rows: Vec<SpaceProgressRow>,
}

#[derive(Template)]
#[template(path = "partials/getting_started.html")]
pub struct GettingStartedTemplate {
    pub show: bool,
    pub total: usize,
    pub completed_count: usize,
    pub items: Vec<(String, bool)>,
}
