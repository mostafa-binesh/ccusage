use std::{
    collections::HashSet,
    env,
    path::{Component, Path, PathBuf},
};

use crate::{Result, collect_files_with_extension};

const QODER_DATA_DIR_ENV: &str = "QODER_DATA_DIR";

pub(super) fn roots() -> Result<Vec<PathBuf>> {
    let candidates = if let Ok(paths) = env::var(QODER_DATA_DIR_ENV) {
        paths
            .split(',')
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .collect::<Vec<_>>()
    } else {
        crate::home::home_dir()
            .map(|home| vec![home.join(".qoder")])
            .unwrap_or_default()
    };
    let mut seen = HashSet::new();
    Ok(candidates
        .into_iter()
        .filter(|path| path.is_dir())
        .filter(|path| seen.insert(path.clone()))
        .collect())
}

pub(super) fn discover_session_files() -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for root in roots()? {
        let sessions = root.join("logs").join("sessions");
        if !sessions.is_dir() {
            continue;
        }
        let mut root_files = Vec::new();
        collect_files_with_extension(&sessions, "jsonl", &mut root_files);
        root_files.retain(|file| is_session_segment(&sessions, file));
        files.extend(root_files);
    }
    files.sort();
    Ok(files)
}

pub(super) fn discover_project_files() -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for root in roots()? {
        let projects = root.join("projects");
        if !projects.is_dir() {
            continue;
        }
        collect_files_with_extension(&projects, "jsonl", &mut files);
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn is_session_segment(sessions: &Path, file: &Path) -> bool {
    let Ok(relative) = file.strip_prefix(sessions) else {
        return false;
    };
    matches!(
        relative.components().collect::<Vec<_>>().as_slice(),
        [Component::Normal(project), Component::Normal(session), Component::Normal(segments), Component::Normal(file)]
            if !project.is_empty()
                && !session.is_empty()
                && segments.to_str() == Some("segments")
                && file.to_string_lossy().ends_with(".jsonl")
    )
}

pub(super) fn session_from_file(file: &Path) -> Option<(String, String)> {
    let parts = file.components().collect::<Vec<_>>();
    for window in parts.windows(6).rev() {
        if let [
            Component::Normal(logs),
            Component::Normal(sessions),
            Component::Normal(project),
            Component::Normal(session),
            Component::Normal(segments),
            Component::Normal(_),
        ] = window
            && logs.to_str() == Some("logs")
            && sessions.to_str() == Some("sessions")
            && segments.to_str() == Some("segments")
        {
            return Some((
                project.to_string_lossy().into_owned(),
                session.to_string_lossy().into_owned(),
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_project_and_session_from_segment_path() {
        let path =
            Path::new("/home/me/.qoder/logs/sessions/project-a/session-a/segments/segment.jsonl");
        assert_eq!(
            session_from_file(path),
            Some(("project-a".to_string(), "session-a".to_string()))
        );
    }
}
