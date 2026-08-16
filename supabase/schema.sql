-- Nook — Supabase (Postgres) schema, applied automatically by `sqlx::migrate!`
-- on startup (see main.rs). Every statement is idempotent (IF NOT EXISTS /
-- OR REPLACE / DROP-then-CREATE) so this is safe to run whether or not
-- `supabase/schema.sql` was already applied by hand in the SQL Editor first —
-- either order ends at the same schema.
--
-- `users` is gone: Supabase Auth owns identity (`auth.users`, UUID); app-
-- specific fields live in `profiles`, created automatically per signup by
-- the trigger below.

-- ---------------------------------------------------------------------------
-- profiles
-- ---------------------------------------------------------------------------
create table if not exists public.profiles (
    id                uuid primary key references auth.users(id) on delete cascade,
    display_name      text not null,
    mode              text not null default 'student'
                          check (mode in ('student', 'work')),
    timezone          text not null default 'UTC',
    theme_preference  text not null default 'system'
                          check (theme_preference in ('light', 'dark', 'system')),
    created_at        timestamptz not null default now()
);

alter table public.profiles enable row level security;

drop policy if exists "profiles are viewable by their owner" on public.profiles;
create policy "profiles are viewable by their owner"
    on public.profiles for select
    using (auth.uid() = id);

drop policy if exists "profiles are editable by their owner" on public.profiles;
create policy "profiles are editable by their owner"
    on public.profiles for update
    using (auth.uid() = id);

create or replace function public.handle_new_user()
returns trigger
language plpgsql
security definer set search_path = public
as $$
begin
    insert into public.profiles (id, display_name, mode)
    values (
        new.id,
        coalesce(new.raw_user_meta_data ->> 'display_name', split_part(new.email, '@', 1)),
        coalesce(new.raw_user_meta_data ->> 'mode', 'student')
    )
    on conflict (id) do nothing;
    return new;
end;
$$;

drop trigger if exists on_auth_user_created on auth.users;
create trigger on_auth_user_created
    after insert on auth.users
    for each row execute procedure public.handle_new_user();

-- ---------------------------------------------------------------------------
-- spaces
-- ---------------------------------------------------------------------------
create table if not exists public.spaces (
    id            bigint generated always as identity primary key,
    user_id       uuid not null references auth.users(id) on delete cascade,
    name          text not null,
    color         text not null
                      check (color in ('clay','sage','slate','amber','plum','teal','rust','denim')),
    icon          text not null default 'folder',
    archived_at   timestamptz,
    created_at    timestamptz not null default now()
);

create index if not exists idx_spaces_user on public.spaces(user_id);
create index if not exists idx_spaces_user_archived on public.spaces(user_id, archived_at);

alter table public.spaces enable row level security;
drop policy if exists "spaces are managed by their owner" on public.spaces;
create policy "spaces are managed by their owner"
    on public.spaces for all
    using (auth.uid() = user_id) with check (auth.uid() = user_id);

-- ---------------------------------------------------------------------------
-- tasks
-- ---------------------------------------------------------------------------
create table if not exists public.tasks (
    id                  bigint generated always as identity primary key,
    space_id            bigint not null references public.spaces(id) on delete cascade,
    title               text not null,
    notes               text not null default '',
    status              text not null default 'todo'
                            check (status in ('todo','in_progress','blocked','done')),
    priority            text not null default 'normal'
                            check (priority in ('low','normal','high')),
    due_at              timestamptz,
    estimated_minutes   integer,
    completed_at        timestamptz,
    last_opened_at      timestamptz,
    logged_minutes      integer not null default 0,
    created_at          timestamptz not null default now(),
    updated_at          timestamptz not null default now()
);

create index if not exists idx_tasks_space on public.tasks(space_id);
create index if not exists idx_tasks_space_status on public.tasks(space_id, status);
create index if not exists idx_tasks_due on public.tasks(due_at);
create index if not exists idx_tasks_status_due on public.tasks(status, due_at);

alter table public.tasks enable row level security;
drop policy if exists "tasks are managed by their space's owner" on public.tasks;
create policy "tasks are managed by their space's owner"
    on public.tasks for all
    using (exists (select 1 from public.spaces where spaces.id = tasks.space_id and spaces.user_id = auth.uid()))
    with check (exists (select 1 from public.spaces where spaces.id = tasks.space_id and spaces.user_id = auth.uid()));

-- ---------------------------------------------------------------------------
-- plan_steps
-- ---------------------------------------------------------------------------
create table if not exists public.plan_steps (
    id        bigint generated always as identity primary key,
    task_id   bigint not null references public.tasks(id) on delete cascade,
    position  integer not null,
    text      text not null,
    done      boolean not null default false
);

create index if not exists idx_plan_steps_task_position on public.plan_steps(task_id, position);

alter table public.plan_steps enable row level security;
drop policy if exists "plan_steps are managed by their task's owner" on public.plan_steps;
create policy "plan_steps are managed by their task's owner"
    on public.plan_steps for all
    using (exists (
        select 1 from public.tasks join public.spaces on spaces.id = tasks.space_id
        where tasks.id = plan_steps.task_id and spaces.user_id = auth.uid()
    ))
    with check (exists (
        select 1 from public.tasks join public.spaces on spaces.id = tasks.space_id
        where tasks.id = plan_steps.task_id and spaces.user_id = auth.uid()
    ));

-- ---------------------------------------------------------------------------
-- attachments
-- ---------------------------------------------------------------------------
create table if not exists public.attachments (
    id                 bigint generated always as identity primary key,
    task_id            bigint not null references public.tasks(id) on delete cascade,
    original_filename  text not null,
    stored_path        text not null unique,
    mime_type          text not null,
    size_bytes         bigint not null,
    uploaded_at        timestamptz not null default now()
);

create index if not exists idx_attachments_task on public.attachments(task_id);

alter table public.attachments enable row level security;
drop policy if exists "attachments are managed by their task's owner" on public.attachments;
create policy "attachments are managed by their task's owner"
    on public.attachments for all
    using (exists (
        select 1 from public.tasks join public.spaces on spaces.id = tasks.space_id
        where tasks.id = attachments.task_id and spaces.user_id = auth.uid()
    ))
    with check (exists (
        select 1 from public.tasks join public.spaces on spaces.id = tasks.space_id
        where tasks.id = attachments.task_id and spaces.user_id = auth.uid()
    ));

-- ---------------------------------------------------------------------------
-- schedule_blocks
-- ---------------------------------------------------------------------------
create table if not exists public.schedule_blocks (
    id             bigint generated always as identity primary key,
    user_id        uuid not null references auth.users(id) on delete cascade,
    space_id       bigint references public.spaces(id) on delete set null,
    title          text not null,
    day_of_week    integer check (day_of_week between 0 and 6),
    start_time     time not null,
    end_time       time not null,
    recurring      boolean not null default true,
    specific_date  date,
    created_at     timestamptz not null default now()
);

create index if not exists idx_schedule_blocks_user on public.schedule_blocks(user_id);
create index if not exists idx_schedule_blocks_user_day on public.schedule_blocks(user_id, day_of_week);
create index if not exists idx_schedule_blocks_user_date on public.schedule_blocks(user_id, specific_date);

alter table public.schedule_blocks enable row level security;
drop policy if exists "schedule_blocks are managed by their owner" on public.schedule_blocks;
create policy "schedule_blocks are managed by their owner"
    on public.schedule_blocks for all
    using (auth.uid() = user_id) with check (auth.uid() = user_id);

-- ---------------------------------------------------------------------------
-- tags / task_tags
-- ---------------------------------------------------------------------------
create table if not exists public.tags (
    id       bigint generated always as identity primary key,
    user_id  uuid not null references auth.users(id) on delete cascade,
    name     text not null,
    unique (user_id, name)
);

alter table public.tags enable row level security;
drop policy if exists "tags are managed by their owner" on public.tags;
create policy "tags are managed by their owner"
    on public.tags for all
    using (auth.uid() = user_id) with check (auth.uid() = user_id);

create table if not exists public.task_tags (
    task_id  bigint not null references public.tasks(id) on delete cascade,
    tag_id   bigint not null references public.tags(id) on delete cascade,
    primary key (task_id, tag_id)
);

create index if not exists idx_task_tags_tag on public.task_tags(tag_id);

alter table public.task_tags enable row level security;
drop policy if exists "task_tags are managed by their task's owner" on public.task_tags;
create policy "task_tags are managed by their task's owner"
    on public.task_tags for all
    using (exists (
        select 1 from public.tasks join public.spaces on spaces.id = tasks.space_id
        where tasks.id = task_tags.task_id and spaces.user_id = auth.uid()
    ))
    with check (exists (
        select 1 from public.tasks join public.spaces on spaces.id = tasks.space_id
        where tasks.id = task_tags.task_id and spaces.user_id = auth.uid()
    ));

-- ---------------------------------------------------------------------------
-- user_layout
-- ---------------------------------------------------------------------------
create table if not exists public.user_layout (
    user_id     uuid primary key references auth.users(id) on delete cascade,
    layout      text not null default '[]',
    updated_at  timestamptz not null default now()
);

-- Safety net: an earlier draft of this schema used `jsonb` for `layout`
-- before the app code (which stores/reads it as a plain JSON-encoded
-- string) settled on `text`. Corrects it in place if that draft already ran.
do $$
begin
    if exists (
        select 1 from information_schema.columns
        where table_schema = 'public' and table_name = 'user_layout'
          and column_name = 'layout' and data_type = 'jsonb'
    ) then
        alter table public.user_layout alter column layout type text using layout::text;
    end if;
end $$;

alter table public.user_layout enable row level security;
drop policy if exists "user_layout is managed by its owner" on public.user_layout;
create policy "user_layout is managed by its owner"
    on public.user_layout for all
    using (auth.uid() = user_id) with check (auth.uid() = user_id);

-- ---------------------------------------------------------------------------
-- onboarding_state
-- ---------------------------------------------------------------------------
create table if not exists public.onboarding_state (
    user_id     uuid primary key references auth.users(id) on delete cascade,
    dismissed   boolean not null default false
);

alter table public.onboarding_state enable row level security;
drop policy if exists "onboarding_state is managed by its owner" on public.onboarding_state;
create policy "onboarding_state is managed by its owner"
    on public.onboarding_state for all
    using (auth.uid() = user_id) with check (auth.uid() = user_id);
