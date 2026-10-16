//! In-memory, read-only proposals over inventory evidence. One item is one
//! observed video, not an application film identity or a media relationship.
use crate::inventory::{CandidateContainer, InventoryReport, Prompt, ScanIssue, VideoEntry};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    SingleVideoCandidate,
    NeedsReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewReason {
    MultipleVideosInDirectory,
    ConflictingCodes,
    RepeatedCodeAcrossDirectories,
    MissingContainerEvidence,
}

#[derive(Debug)]
pub struct VideoProposal<'a> {
    pub video: &'a VideoEntry,
    pub disposition: Disposition,
    pub reasons: Vec<ReviewReason>,
    /// Other observed files in the same directory; no relationship is assigned.
    pub peer_video_paths: Vec<&'a str>,
    pub adjacent_asset_paths: Vec<&'a str>,
    pub container_prompts: Vec<Prompt>,
}

#[derive(Debug)]
pub struct ProposalReport<'a> {
    pub items: Vec<VideoProposal<'a>>,
    pub issues: &'a [ScanIssue],
}

pub fn propose(inventory: &InventoryReport) -> ProposalReport<'_> {
    let containers: BTreeMap<&str, &CandidateContainer> = inventory
        .containers
        .iter()
        .map(|container| (container.directory.as_str(), container))
        .collect();
    let items = inventory
        .videos
        .iter()
        .map(|video| {
            let container = containers.get(video.directory.as_str()).copied();
            let mut reasons = Vec::new();
            match container {
                Some(container) => {
                    if container.video_paths.len() > 1 {
                        reasons.push(ReviewReason::MultipleVideosInDirectory);
                    }
                    if container.prompts.contains(&Prompt::ConflictingCodes) {
                        reasons.push(ReviewReason::ConflictingCodes);
                    }
                    if container
                        .prompts
                        .contains(&Prompt::RepeatedCodeAcrossDirectories)
                    {
                        reasons.push(ReviewReason::RepeatedCodeAcrossDirectories);
                    }
                }
                None => reasons.push(ReviewReason::MissingContainerEvidence),
            }
            let disposition = if reasons.is_empty() {
                Disposition::SingleVideoCandidate
            } else {
                Disposition::NeedsReview
            };
            VideoProposal {
                video,
                disposition,
                reasons,
                peer_video_paths: container
                    .map(|c| {
                        c.video_paths
                            .iter()
                            .filter(|path| *path != &video.relative_path)
                            .map(String::as_str)
                            .collect()
                    })
                    .unwrap_or_default(),
                adjacent_asset_paths: container
                    .map(|c| c.asset_paths.iter().map(String::as_str).collect())
                    .unwrap_or_default(),
                container_prompts: container.map(|c| c.prompts.clone()).unwrap_or_default(),
            }
        })
        .collect();
    ProposalReport {
        items,
        issues: &inventory.issues,
    }
}
