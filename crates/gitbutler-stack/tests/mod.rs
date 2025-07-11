mod file_ownership;
mod ownership;

use anyhow::Result;
use but_core::Reference;
use gitbutler_command_context::CommandContext;
use gitbutler_oxidize::{ObjectIdExt, OidExt};
use gitbutler_repo::logging::{LogUntil, RepositoryExt as _};
use gitbutler_repo_actions::RepoActionsExt;
use gitbutler_stack::PatchReferenceUpdate;
use gitbutler_stack::{CommitOrChangeId, StackBranch, VirtualBranchesHandle};
use itertools::Itertools;
use tempfile::TempDir;

#[test]
fn add_series_success() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let reference = StackBranch::new(
        CommitOrChangeId::CommitId(test_ctx.commits[1].id().to_string()),
        "asdf".into(),
        Some("my description".into()),
        &ctx.gix_repo()?,
    )?;
    let result = test_ctx.stack.add_series(&ctx, reference, None);
    assert!(result.is_ok());
    assert_eq!(test_ctx.stack.heads.len(), 2);
    assert_eq!(test_ctx.stack.heads[0].name(), "asdf");
    assert_eq!(
        test_ctx.stack.heads[0].description,
        Some("my description".into())
    );
    // Assert persisted
    assert_eq!(
        test_ctx.stack,
        test_ctx.handle.get_stack(test_ctx.stack.id)?
    );
    Ok(())
}

#[test]
fn add_series_top_of_stack() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let result =
        test_ctx
            .stack
            .add_series_top_of_stack(&ctx, "asdf".into(), Some("my description".into()));
    assert!(result.is_ok());
    assert_eq!(test_ctx.stack.heads.len(), 2);
    assert_eq!(test_ctx.stack.heads[1].name(), "asdf");
    assert_eq!(
        test_ctx.stack.heads[1].description,
        Some("my description".into())
    );
    // Assert persisted
    assert_eq!(
        test_ctx.stack,
        test_ctx.handle.get_stack(test_ctx.stack.id)?
    );
    Ok(())
}

#[test]
fn add_series_top_base() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let merge_base = ctx.repo().find_commit(ctx.repo().merge_base(
        test_ctx.stack.head_oid(&ctx.gix_repo()?)?.to_git2(),
        test_ctx.default_target.sha,
    )?)?;
    let reference = StackBranch::new(
        CommitOrChangeId::CommitId(merge_base.id().to_string()),
        "asdf".into(),
        Some("my description".into()),
        &ctx.gix_repo()?,
    )?;
    let result = test_ctx.stack.add_series(&ctx, reference, None);
    println!("{:?}", result);
    // Assert persisted
    assert_eq!(
        test_ctx.stack,
        test_ctx.handle.get_stack(test_ctx.stack.id)?
    );
    Ok(())
}

#[test]
fn add_multiple_series() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;

    assert_eq!(test_ctx.stack.heads.len(), 1);
    assert_eq!(head_names(&test_ctx), vec!["virtual"]); // defaults to stack name
    let default_head = test_ctx.stack.heads[0].clone();

    let head_4 = StackBranch::new(
        CommitOrChangeId::CommitId(test_ctx.commits.last().unwrap().id().to_string()),
        "head_4".into(),
        None,
        &ctx.gix_repo()?,
    )?;
    let result = test_ctx
        .stack
        .add_series(&ctx, head_4, Some(default_head.name().clone()));
    assert!(result.is_ok());
    assert_eq!(head_names(&test_ctx), vec!["virtual", "head_4"]);

    let head_2 = StackBranch::new(
        CommitOrChangeId::CommitId(test_ctx.commits.last().unwrap().id().to_string()),
        "head_2".into(),
        None,
        &ctx.gix_repo()?,
    )?;
    let result = test_ctx.stack.add_series(&ctx, head_2, None);
    assert!(result.is_ok());
    assert_eq!(head_names(&test_ctx), vec!["head_2", "virtual", "head_4"]);

    let head_1 = StackBranch::new(
        CommitOrChangeId::CommitId(test_ctx.commits.first().unwrap().id().to_string()),
        "head_1".into(),
        None,
        &ctx.gix_repo()?,
    )?;

    let result = test_ctx.stack.add_series(&ctx, head_1, None);
    assert!(result.is_ok());
    assert_eq!(
        head_names(&test_ctx),
        vec!["head_1", "head_2", "virtual", "head_4"]
    );

    // archive is noop
    let before_prune = test_ctx.stack.heads.clone();
    test_ctx
        .stack
        .archive_integrated_heads(&ctx, &ctx.gix_repo()?, &[], false)?;
    assert_eq!(before_prune, test_ctx.stack.heads);
    Ok(())
}

#[test]
fn add_series_invalid_name_fails() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let test_ctx = test_ctx(&ctx)?;
    let result = StackBranch::new(
        CommitOrChangeId::CommitId(test_ctx.commits[0].id().to_string()),
        "name with spaces".into(),
        None,
        &ctx.gix_repo()?,
    );
    assert_eq!(
        result.err().unwrap().to_string(),
        "A reference must be a valid tag name as well"
    );
    Ok(())
}

#[test]
fn add_series_duplicate_name_fails() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let reference = StackBranch::new(
        CommitOrChangeId::CommitId(test_ctx.commits[1].id().to_string()),
        "asdf".into(),
        None,
        &ctx.gix_repo()?,
    )?;
    let result = test_ctx.stack.add_series(&ctx, reference.clone(), None);
    assert!(result.is_ok());
    let result = test_ctx.stack.add_series(&ctx, reference, None);
    assert_eq!(
        result.err().unwrap().to_string(),
        "A patch reference with the name asdf exists"
    );
    Ok(())
}

#[test]
fn add_series_matching_git_ref_is_ok() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let reference = StackBranch::new(
        test_ctx.commits[0].parent(0)?.clone(),
        "existing-branch".into(),
        None,
        &ctx.gix_repo()?,
    )?;
    let result = test_ctx.stack.add_series(&ctx, reference.clone(), None);
    assert!(result.is_ok()); // allow this
    Ok(())
}

#[test]
fn add_series_including_refs_head_fails() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let reference = StackBranch::new(
        CommitOrChangeId::CommitId(test_ctx.commits[0].id().to_string()),
        "refs/heads/my-branch".into(),
        None,
        &ctx.gix_repo()?,
    )?;
    let result = test_ctx.stack.add_series(&ctx, reference.clone(), None);
    assert_eq!(
        result.err().unwrap().to_string(),
        "Stack head name cannot start with 'refs/heads'"
    );
    Ok(())
}

#[test]
fn add_series_target_commit_doesnt_exist() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let reference = StackBranch::new(
        CommitOrChangeId::CommitId("30696678319e0fa3a20e54f22d47fc8cf1ceaade".into()), // does not exist
        "my-branch".into(),
        None,
        &ctx.gix_repo()?,
    )?;
    let result = test_ctx.stack.add_series(&ctx, reference.clone(), None);
    assert_eq!(
        result.err().unwrap().to_string(),
        "An object with id 30696678319e0fa3a20e54f22d47fc8cf1ceaade could not be found",
    );
    Ok(())
}

#[test]
fn add_series_target_change_id_doesnt_exist() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let reference = StackBranch::new(
        CommitOrChangeId::CommitId("10696678319e0fa3a20e54f22d47fc8cf1ceaade".into()), // does not exist
        "my-branch".into(),
        None,
        &ctx.gix_repo()?,
    )?;
    let result = test_ctx.stack.add_series(&ctx, reference.clone(), None);
    assert_eq!(
        result.err().unwrap().to_string(),
        "An object with id 10696678319e0fa3a20e54f22d47fc8cf1ceaade could not be found",
    );
    Ok(())
}

#[test]
fn add_series_target_commit_not_in_stack() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let other_commit_id = test_ctx.other_commits.last().unwrap().id().to_string();
    let reference = StackBranch::new(
        CommitOrChangeId::CommitId(other_commit_id.clone()), // does not exist
        "my-branch".into(),
        None,
        &ctx.gix_repo()?,
    )?;
    let result = test_ctx.stack.add_series(&ctx, reference.clone(), None);
    assert_eq!(
        result.err().unwrap().to_string(),
        format!(
            "The commit {} is not between the stack head and the stack base",
            other_commit_id
        )
    );
    Ok(())
}

#[test]
fn remove_branch_last_fails() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let result = test_ctx
        .stack
        .remove_branch(&ctx, test_ctx.stack.heads[0].name().clone());
    assert_eq!(
        result.err().unwrap().to_string(),
        "Cannot remove the last branch from the stack"
    );
    Ok(())
}

#[test]
fn remove_branch_nonexistent_fails() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let result = test_ctx
        .stack
        .remove_branch(&ctx, "does-not-exist".to_string());
    assert_eq!(
        result.err().unwrap().to_string(),
        "Series with name does-not-exist not found"
    );
    Ok(())
}

#[test]
fn remove_branch_with_multiple_last_heads() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;

    assert_eq!(test_ctx.stack.heads.len(), 1);
    assert_eq!(head_names(&test_ctx), vec!["virtual"]); // defaults to stack name
    let default_head = test_ctx.stack.heads[0].clone();
    let repo = &ctx.gix_repo()?;
    let to_stay = StackBranch::new(
        CommitOrChangeId::CommitId(test_ctx.commits.last().unwrap().id().to_string()),
        "to_stay".into(),
        None,
        repo,
    )?;
    let result = test_ctx.stack.add_series(&ctx, to_stay.clone(), None);
    assert!(result.is_ok());
    assert_eq!(head_names(&test_ctx), vec!["to_stay", "virtual"]);

    let result = test_ctx
        .stack
        .remove_branch(&ctx, default_head.name().clone());
    assert!(result.is_ok());
    assert_eq!(head_names(&test_ctx), vec!["to_stay"]);
    assert_eq!(
        CommitOrChangeId::CommitId(test_ctx.commits.last().unwrap().id().to_string()),
        test_ctx.stack.heads[0].head_oid(repo)?.into(),
    ); // it references the newest commit
    Ok(())
}

#[test]
fn remove_branch_no_orphan_commits() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;

    assert_eq!(test_ctx.stack.heads.len(), 1);
    assert_eq!(head_names(&test_ctx), vec!["virtual"]); // defaults to stack name
    let default_head = test_ctx.stack.heads[0].clone(); // references the newest commit

    let repo = &ctx.gix_repo()?;
    let to_stay = StackBranch::new(
        CommitOrChangeId::CommitId(test_ctx.commits.first().unwrap().id().to_string()),
        "to_stay".into(),
        None,
        repo,
    )?; // references the oldest commit
    let result = test_ctx.stack.add_series(&ctx, to_stay.clone(), None);
    assert!(result.is_ok());
    assert_eq!(head_names(&test_ctx), vec!["to_stay", "virtual"]);

    let result = test_ctx
        .stack
        .remove_branch(&ctx, default_head.name().clone());
    assert!(result.is_ok());
    assert_eq!(head_names(&test_ctx), vec!["to_stay"]);
    assert_eq!(
        CommitOrChangeId::CommitId(test_ctx.commits.last().unwrap().id().to_string()),
        test_ctx.stack.heads[0].head_oid(repo)?.into()
    ); // it was updated to reference the newest commit
    Ok(())
}

#[test]
fn update_series_noop_does_nothing() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let heads_before = test_ctx.stack.heads.clone();
    let noop_update = PatchReferenceUpdate::default();
    let result = test_ctx
        .stack
        .update_branch(&ctx, "virtual".into(), &noop_update);
    assert!(result.is_ok());
    assert_eq!(test_ctx.stack.heads, heads_before);
    Ok(())
}

#[test]
fn update_branch_name_fails_validation() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let update = PatchReferenceUpdate {
        name: Some("invalid name".into()),
        description: None,
    };
    let result = test_ctx
        .stack
        .update_branch(&ctx, "virtual".into(), &update);
    assert_eq!(result.err().unwrap().to_string(), "Invalid branch name");
    Ok(())
}

#[test]
fn update_branch_name_success() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let update = PatchReferenceUpdate {
        name: Some("new-name".into()),
        description: None,
    };
    let result = test_ctx
        .stack
        .update_branch(&ctx, "virtual".into(), &update);
    assert!(result.is_ok());
    assert_eq!(test_ctx.stack.heads[0].name(), "new-name");
    // Assert persisted
    assert_eq!(
        test_ctx.stack,
        test_ctx.handle.get_stack(test_ctx.stack.id)?
    );
    Ok(())
}

#[test]
fn update_branch_name_resets_pr_number() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let pr_number = 123;
    test_ctx
        .stack
        .set_pr_number(&ctx, "virtual", Some(pr_number))?;
    assert_eq!(test_ctx.stack.heads[0].pr_number, Some(pr_number));
    let update = PatchReferenceUpdate {
        name: Some("new-name".into()),
        description: None,
    };
    test_ctx
        .stack
        .update_branch(&ctx, "virtual".into(), &update)?;
    assert_eq!(test_ctx.stack.heads[0].pr_number, None);
    assert_eq!(
        test_ctx.stack,
        test_ctx.handle.get_stack(test_ctx.stack.id)?
    );
    Ok(())
}

#[test]
fn update_series_set_description() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let update = PatchReferenceUpdate {
        name: None,
        description: Some(Some("my description".into())),
    };
    let result = test_ctx
        .stack
        .update_branch(&ctx, "virtual".into(), &update);
    assert!(result.is_ok());
    assert_eq!(
        test_ctx.stack.heads[0].description,
        Some("my description".into())
    );
    // Assert persisted
    assert_eq!(
        test_ctx.stack,
        test_ctx.handle.get_stack(test_ctx.stack.id)?
    );
    Ok(())
}

#[test]
fn push_series_success() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let test_ctx = test_ctx(&ctx)?;

    let state = VirtualBranchesHandle::new(ctx.project().gb_dir());
    let mut target = state.get_default_target()?;
    target.push_remote_name = Some("origin".into());
    state.set_default_target(target)?;

    let result = test_ctx.stack.push_details(&ctx, "virtual".into());
    assert!(result.is_ok());
    Ok(())
}

#[test]
fn update_name_after_push() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;

    let state = VirtualBranchesHandle::new(ctx.project().gb_dir());
    let mut target = state.get_default_target()?;
    target.push_remote_name = Some("origin".into());
    state.set_default_target(target)?;

    let push_details = test_ctx.stack.push_details(&ctx, "virtual".into())?;
    let result = ctx.push(
        push_details.head,
        &push_details.remote_refname,
        false,
        None,
        Some(Some(test_ctx.stack.id)),
    );
    assert!(result.is_ok());
    let result = test_ctx.stack.update_branch(
        &ctx,
        "virtual".into(),
        &PatchReferenceUpdate {
            name: Some("new-name".into()),
            ..Default::default()
        },
    );
    assert!(result.is_ok());
    Ok(())
}

#[test]
fn list_series_default_head() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let test_ctx = test_ctx(&ctx)?;
    let branches = test_ctx.stack.branches();
    // the number of series matches the number of heads
    assert_eq!(branches.len(), test_ctx.stack.heads.len());
    assert_eq!(branches[0].name(), "virtual");
    assert_eq!(
        branches[0]
            .commits(&ctx, &test_ctx.stack)?
            .local_commits
            .iter()
            .map(|c| c.id())
            .collect_vec(),
        test_ctx.commits.iter().map(|c| c.id()).collect_vec()
    );
    Ok(())
}

#[test]
fn list_series_two_heads_same_commit() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let head_before = StackBranch::new(
        CommitOrChangeId::CommitId(test_ctx.commits.last().unwrap().id().to_string()),
        "head_before".into(),
        None,
        &ctx.gix_repo()?,
    )?;
    // add `head_before` before the initial head
    let result = test_ctx.stack.add_series(&ctx, head_before, None);
    assert!(result.is_ok());

    let branches = test_ctx.stack.branches();

    // the number of series matches the number of heads
    assert_eq!(branches.len(), test_ctx.stack.heads.len());

    assert_eq!(
        branches[0]
            .commits(&ctx, &test_ctx.stack)?
            .local_commits
            .iter()
            .map(|c| c.id())
            .collect_vec(),
        test_ctx.commits.iter().map(|c| c.id()).collect_vec()
    );
    assert_eq!(branches[0].name(), "head_before");
    assert_eq!(
        branches[1]
            .commits(&ctx, &test_ctx.stack)?
            .local_commits
            .iter()
            .map(|c| c.id())
            .collect_vec(),
        vec![]
    );
    assert_eq!(branches[1].name(), "virtual");
    Ok(())
}

#[test]
fn list_series_two_heads_different_commit() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let head_before = StackBranch::new(
        CommitOrChangeId::CommitId(test_ctx.commits.first().unwrap().id().to_string()),
        "head_before".into(),
        None,
        &ctx.gix_repo()?,
    )?;

    // add `head_before` before the initial head
    let result = test_ctx.stack.add_series(&ctx, head_before, None);
    assert!(result.is_ok());
    let branches = test_ctx.stack.branches();
    // the number of series matches the number of heads
    assert_eq!(branches.len(), test_ctx.stack.heads.len());
    let mut expected_patches = test_ctx.commits.iter().map(|c| c.id()).collect_vec();
    assert_eq!(
        branches[0]
            .commits(&ctx, &test_ctx.stack)?
            .local_commits
            .iter()
            .map(|c| c.id())
            .collect_vec(),
        vec![expected_patches.remove(0)]
    );
    assert_eq!(branches[0].name(), "head_before");
    assert_eq!(expected_patches.len(), 2);
    assert_eq!(
        branches[1]
            .commits(&ctx, &test_ctx.stack)?
            .local_commits
            .iter()
            .map(|c| c.id())
            .collect_vec(),
        expected_patches
    ); // the other two patches are in the second series
    assert_eq!(branches[1].name(), "virtual");

    Ok(())
}

#[test]
fn set_stack_head_commit_invalid() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let vb_state = VirtualBranchesHandle::new(ctx.project().gb_dir());
    let gix_repo = ctx.gix_repo()?;
    let result = test_ctx
        .stack
        .set_stack_head(&vb_state, &gix_repo, git2::Oid::zero(), None);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn set_stack_head() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let commit = test_ctx.other_commits.last().unwrap();
    let vb_state = VirtualBranchesHandle::new(ctx.project().gb_dir());
    let gix_repo = ctx.gix_repo()?;
    let result = test_ctx
        .stack
        .set_stack_head(&vb_state, &gix_repo, commit.id(), None);
    assert!(result.is_ok());
    let branches = test_ctx.stack.branches();
    assert_eq!(
        CommitOrChangeId::CommitId(commit.id().to_string()),
        branches.first().unwrap().head_oid(&ctx.gix_repo()?)?.into()
    );
    assert_eq!(
        test_ctx.stack.head_oid(&ctx.gix_repo()?)?,
        test_ctx.other_commits.last().unwrap().id().to_gix()
    );
    Ok(())
}

#[test]
fn archive_heads_noop() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let initial_state = test_ctx.stack.heads.clone();
    test_ctx
        .stack
        .archive_integrated_heads(&ctx, &ctx.gix_repo()?, &[], false)?;
    assert_eq!(initial_state, test_ctx.stack.heads);
    // Assert persisted
    assert_eq!(
        test_ctx.stack,
        test_ctx.handle.get_stack(test_ctx.stack.id)?
    );
    Ok(())
}

#[test]
fn archive_heads_success() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    // adding a commit that is not in the stack
    test_ctx.stack.heads.insert(
        0,
        StackBranch::new(
            test_ctx.other_commits.first().cloned().unwrap(),
            "foo".to_string(),
            None,
            &ctx.gix_repo()?,
        )?,
    );
    assert_eq!(test_ctx.stack.heads.len(), 2);
    test_ctx.stack.archive_integrated_heads(
        &ctx,
        &ctx.gix_repo()?,
        &[Reference::Virtual("foo".to_string())],
        false,
    )?;
    assert_eq!(test_ctx.stack.heads.len(), 2);
    assert!(test_ctx.stack.heads[0].archived);
    assert!(!test_ctx.stack.heads[1].archived);
    // Assert persisted
    assert_eq!(
        test_ctx.stack,
        test_ctx.handle.get_stack(test_ctx.stack.id)?
    );
    Ok(())
}

// #[test]
// fn does_not_archive_head_on_merge_base() -> Result<()> {
//     let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
//     let mut test_ctx = test_ctx(&ctx)?;
//     let merge_base = ctx.repository().find_commit(
//         ctx.repository()
//             .merge_base(test_ctx.stack.head(), test_ctx.default_target.sha)?,
//     )?;
//     test_ctx.stack.add_series(
//         &ctx,
//         StackBranch {
//             head: merge_base.into(),
//             name: "bottom".to_string(),
//             description: None,
//             pr_number: Default::default(),
//             archived: Default::default(),
//         },
//         None,
//     )?;
//     let initial_state = test_ctx.stack.heads.clone();
//     test_ctx.stack.archive_integrated_heads(&ctx)?;
//     assert_eq!(initial_state, test_ctx.stack.heads);
//     // Assert persisted
//     assert_eq!(
//         test_ctx.stack,
//         test_ctx.handle.get_stack(test_ctx.stack.id)?
//     );
//     Ok(())
// }

#[test]
fn set_pr_numberentifiers_success() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let result = test_ctx.stack.set_pr_number(&ctx, "virtual", Some(123));
    assert!(result.is_ok());
    assert_eq!(test_ctx.stack.heads[0].pr_number, Some(123));
    // Assert persisted
    assert_eq!(
        test_ctx.stack,
        test_ctx.handle.get_stack(test_ctx.stack.id)?
    );
    Ok(())
}

#[test]
fn set_pr_numberentifiers_series_not_found_fails() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let mut test_ctx = test_ctx(&ctx)?;
    let result = test_ctx
        .stack
        .set_pr_number(&ctx, "does-not-exist", Some(123));
    assert_eq!(
        result.err().unwrap().to_string(),
        format!(
            "Series does-not-exist does not exist on stack {}",
            test_ctx.stack.name
        )
    );
    Ok(())
}

#[test]
fn add_head_with_archived_bottom_head() -> Result<()> {
    let (ctx, _temp_dir) = command_ctx("multiple-commits")?;
    let test_ctx = test_ctx(&ctx)?;
    let mut head_1_archived = StackBranch::new(
        test_ctx.commits[0].clone(),
        "kv-branch-3".to_string(),
        None,
        &ctx.gix_repo()?,
    )?;
    head_1_archived.archived = true;
    let head_2 = StackBranch::new(
        test_ctx.commits[1].clone(),
        "more-on-top".to_string(),
        None,
        &ctx.gix_repo()?,
    )?;
    let existing_heads = vec![head_1_archived.clone(), head_2.clone()];
    let new_head = StackBranch::new(
        test_ctx.commits[1].clone(), // same as head_2
        "abcd".to_string(),
        None,
        &ctx.gix_repo()?,
    )?;
    let patches: Vec<CommitOrChangeId> = vec![
        test_ctx.commits[0].clone().into(),
        test_ctx.commits[1].clone().into(),
    ];

    let updated_heads = gitbutler_stack::add_head(
        existing_heads,
        new_head.clone(),
        Some(head_2.clone()),
        patches,
        &ctx.gix_repo()?,
    )?;
    assert_eq!(updated_heads, vec![head_1_archived, head_2, new_head]);
    Ok(())
}

fn command_ctx(name: &str) -> Result<(CommandContext, TempDir)> {
    gitbutler_testsupport::writable::fixture("stacking.sh", name)
}

fn head_names(test_ctx: &TestContext) -> Vec<String> {
    test_ctx
        .stack
        .heads
        .iter()
        .map(|h| h.name().clone())
        .collect_vec()
}

fn test_ctx(ctx: &CommandContext) -> Result<TestContext> {
    let handle = VirtualBranchesHandle::new(ctx.project().gb_dir());
    let stacks = handle.list_all_stacks()?;
    let stack = stacks.iter().find(|b| b.name == "virtual").unwrap();
    let other_stack = stacks.iter().find(|b| b.name != "virtual").unwrap();
    let target = handle.get_default_target()?;
    let gix_repo = ctx.gix_repo()?;
    let mut branch_commits = ctx.repo().log(
        stack.head_oid(&gix_repo)?.to_git2(),
        LogUntil::Commit(target.sha),
        false,
    )?;
    branch_commits.reverse();
    let mut other_commits = ctx.repo().log(
        other_stack.head_oid(&gix_repo)?.to_git2(),
        LogUntil::Commit(target.sha),
        false,
    )?;
    other_commits.reverse();
    Ok(TestContext {
        stack: stack.clone(),
        commits: branch_commits,
        // other_branch: other_branch.clone(),
        other_commits,
        handle,
        default_target: target,
    })
}
struct TestContext<'a> {
    stack: gitbutler_stack::Stack,
    /// Oldest commit first
    commits: Vec<git2::Commit<'a>>,
    /// Oldest commit first
    #[allow(dead_code)]
    other_commits: Vec<git2::Commit<'a>>,
    handle: VirtualBranchesHandle,
    default_target: gitbutler_stack::Target,
}
