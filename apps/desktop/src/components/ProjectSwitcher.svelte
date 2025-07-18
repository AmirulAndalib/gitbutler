<script lang="ts">
	import { goto } from '$app/navigation';
	import { ProjectsService } from '$lib/project/projectsService';
	import { getContext } from '@gitbutler/shared/context';
	import Button from '@gitbutler/ui/Button.svelte';
	import OptionsGroup from '@gitbutler/ui/select/OptionsGroup.svelte';
	import Select from '@gitbutler/ui/select/Select.svelte';
	import SelectItem from '@gitbutler/ui/select/SelectItem.svelte';

	const { projectId }: { projectId?: string } = $props();

	const projectsService = getContext(ProjectsService);
	const projectsResult = $derived(projectsService.projects());

	let selectedId = $state<string>();

	const mappedProjects = $derived(
		projectsResult.current?.data?.map((project) => ({
			value: project.id,
			label: project.title
		})) || []
	);

	let newProjectLoading = $state(false);
	let cloneProjectLoading = $state(false);
</script>

<div class="project-switcher">
	<Select
		value={projectId}
		options={mappedProjects}
		label="Switch to another project"
		wide
		onselect={(value) => {
			selectedId = value;
		}}
		searchable
	>
		{#snippet itemSnippet({ item, highlighted })}
			<SelectItem selected={item.value === projectId} {highlighted}>
				{item.label}
			</SelectItem>
		{/snippet}

		<OptionsGroup>
			<SelectItem
				icon="plus"
				loading={newProjectLoading}
				onClick={async () => {
					newProjectLoading = true;
					try {
						await projectsService.addProject();
					} finally {
						newProjectLoading = false;
					}
				}}
			>
				Add local repository
			</SelectItem>
			<SelectItem
				icon="clone"
				loading={cloneProjectLoading}
				onClick={async () => {
					cloneProjectLoading = true;
					try {
						goto('/onboarding/clone');
					} finally {
						cloneProjectLoading = false;
					}
				}}
			>
				Clone repository
			</SelectItem>
		</OptionsGroup>
	</Select>

	<Button
		style="pop"
		icon="chevron-right-small"
		disabled={selectedId === projectId}
		onmousedown={() => {
			if (projectId) goto(`/${projectId}/`);
		}}
	>
		Open project
	</Button>
</div>

<style lang="postcss">
	.project-switcher {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 10px;
	}
</style>
