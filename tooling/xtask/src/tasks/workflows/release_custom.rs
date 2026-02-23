use crate::tasks::workflows::{
    release::{ReleaseBundleJobs, download_workflow_artifacts, prep_release_artifacts},
    run_bundling::{bundle_linux, bundle_mac, bundle_windows},
    runners::{Arch, ReleaseChannel},
    steps::{FluentBuilder, NamedJob},
};

use super::{runners, steps, steps::named, vars::WorkflowInput};
use gh_workflow::*;
use indoc::indoc;

/// Generates the release_custom.yml workflow for manual releases with custom tag
pub fn release_custom() -> Workflow {
    let tag_input = WorkflowInput::string("tag", Some("nightly".to_string()))
        .description("Git tag name for the release");

    let nightly = Some(ReleaseChannel::Nightly);
    let bundle = ReleaseBundleJobs {
        linux_aarch64: bundle_linux(Arch::AARCH64, nightly, &[]),
        linux_x86_64: bundle_linux(Arch::X86_64, nightly, &[]),
        mac_aarch64: bundle_mac(Arch::AARCH64, nightly, &[]),
        mac_x86_64: bundle_mac(Arch::X86_64, nightly, &[]),
        windows_aarch64: bundle_windows(Arch::AARCH64, nightly, &[]),
        windows_x86_64: bundle_windows(Arch::X86_64, nightly, &[]),
    };

    let update_release_tag = update_release_tag_job(&bundle);

    named::workflow()
        .on(Event::default().workflow_dispatch(
            WorkflowDispatch::default().add_input(tag_input.name, tag_input.input()),
        ))
        .add_env(("CARGO_TERM_COLOR", "always"))
        .add_env(("RUST_BACKTRACE", "1"))
        .map(|mut workflow| {
            for job in bundle.into_jobs() {
                workflow = workflow.add_job(job.name, job.job);
            }
            workflow
        })
        .add_job(update_release_tag.name, update_release_tag.job)
}

fn update_release_tag_job(bundle: &ReleaseBundleJobs) -> NamedJob {
    fn set_release_channel() -> Step<Run> {
        named::bash(indoc! {r#"
            echo "nightly" > crates/zed/RELEASE_CHANNEL
        "#})
    }

    fn upload_to_github_release() -> Step<Run> {
        named::bash(indoc! {r#"
            TAG="${{ github.event.inputs.tag }}"
            [ -z "$TAG" ] && TAG="nightly"

            # Delete all existing assets from release to keep only latest
            echo "Deleting existing assets from $TAG release..."
            gh release view "$TAG" --json assets --jq '.assets[].name' 2>/dev/null | while read asset; do
              echo "Deleting $asset..."
              gh release delete-asset "$TAG" "$asset" --yes || true
            done

            # Upload all new release artifacts to GitHub Releases
            for file in ./release-artifacts/*; do
              if [ -f "$file" ]; then
                echo "Uploading $file to GitHub Release..."
                gh release upload "$TAG" "$file" || true
              fi
            done
        "#})
    }

    NamedJob {
        name: "update_release_tag".to_owned(),
        job: steps::release_job(&bundle.jobs())
            .runs_on(runners::LINUX_MEDIUM)
            .timeout_minutes(180u32)
            .add_step(steps::checkout_repo().with_full_history())
            .add_step(set_release_channel())
            .add_step(download_workflow_artifacts())
            .add_step(steps::script("ls -lR ./artifacts"))
            .add_step(prep_release_artifacts())
            .add_step(upload_to_github_release()),
    }
}
