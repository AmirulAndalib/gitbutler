<script lang="ts">
	import { BranchStatus, type Branch } from '@gitbutler/shared/branches/types';
	import { CommitStatusBadge } from '@gitbutler/ui';

	import type { PatchCommit } from '$lib/patches/types';

	type Props = {
		branch: Branch;
	};

	const { branch }: Props = $props();

	const patches = $derived(branch.patches);
	const anyRejected = $derived(
		patches?.some((patch: PatchCommit) => patch.reviewAll.rejected.length > 0)
	);
	const someApproved = $derived(
		patches?.some((patch: PatchCommit) => patch.reviewAll.signedOff.length > 0)
	);
	const allApproved = $derived(
		!patches?.some((patch: PatchCommit) => patch.reviewAll.signedOff.length === 0)
	);
	const hasComments = $derived(
		patches?.some((patch: PatchCommit) => patch.commentCount > 0) ?? false
	);

	const status = $derived.by(() => {
		if (branch.status === BranchStatus.Closed) {
			return 'closed';
		} else if (branch.status === BranchStatus.Loading) {
			return 'loading';
		} else if (anyRejected) {
			return 'changes-requested';
		} else if (allApproved) {
			return 'approved';
		} else if (someApproved) {
			return 'in-discussion';
		} else if (hasComments && !someApproved && !anyRejected) {
			return 'in-discussion';
		} else {
			return 'unreviewed';
		}
	});
</script>

<CommitStatusBadge {status} />
