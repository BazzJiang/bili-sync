<script lang="ts">
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Checkbox } from '$lib/components/ui/checkbox/index.js';
	import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import * as Tooltip from '$lib/components/ui/tooltip/index.js';
	import type { VideoInfo } from '$lib/types';
	import BrushCleaningIcon from '@lucide/svelte/icons/brush-cleaning';
	import ClockIcon from '@lucide/svelte/icons/clock';
	import EllipsisIcon from '@lucide/svelte/icons/ellipsis';
	import FolderIcon from '@lucide/svelte/icons/folder';
	import HeartIcon from '@lucide/svelte/icons/heart';
	import InfoIcon from '@lucide/svelte/icons/info';
	import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
	import SquareArrowOutUpRightIcon from '@lucide/svelte/icons/square-arrow-out-up-right';
	import UserIcon from '@lucide/svelte/icons/user';
	import { goto } from '$app/navigation';

	// 列表行：与 video-card 保持同一套状态语义，只是把卡片换成横向一行
	export let video: Omit<VideoInfo, 'bvid'> & { bvid?: string };
	export let source: { type: string; name: string } | null = null;
	export let onReset: ((forceReset: boolean) => Promise<void>) | null = null;
	export let onClearAndReset: (() => Promise<void>) | null = null;
	// 批量选择（仅 /videos 列表开启）
	export let selectable: boolean = false;
	export let selected: boolean = false;
	export let onToggleSelect: ((checked: boolean) => void) | null = null;

	let resetDialogOpen = false;
	let clearAndResetDialogOpen = false;
	let resetting = false;
	let clearAndResetting = false;
	let forceReset = false;

	function getStatusText(status: number): string {
		if (status === 7) {
			return '已完成';
		} else if (status === 0) {
			return '未开始';
		} else {
			return `失败${status}次`;
		}
	}

	function getSegmentColor(status: number): string {
		if (status === 7) {
			return 'bg-emerald-500';
		} else if (status === 0) {
			return 'bg-yellow-500';
		} else {
			return 'bg-rose-500';
		}
	}

	function getOverallStatus(
		downloadStatus: number[],
		shouldDownload: boolean,
		valid: boolean
	): {
		text: string;
		style: string;
	} {
		if (!valid) {
			// 视频属性表明已失效，或由于各种条件判断（充电视频等）判定为无效的情况
			return { text: '失效', style: 'bg-gray-100 text-gray-700' };
		}
		if (!shouldDownload) {
			// 被过滤规则排除，显示为“跳过”
			return { text: '跳过', style: 'bg-gray-100 text-gray-700' };
		}
		const completed = downloadStatus.filter((status) => status === 7).length;
		const total = downloadStatus.length;
		const failed = downloadStatus.filter((status) => status !== 7 && status !== 0).length;

		if (completed === total) {
			// 全部完成，显示为“完成”
			return { text: '完成', style: 'bg-emerald-700 text-emerald-100' };
		} else if (failed > 0) {
			// 出现了失败，显示为“失败”
			return { text: '失败', style: 'bg-rose-700 text-rose-100' };
		} else {
			// 还未开始，显示为“等待”
			return { text: '等待', style: 'bg-yellow-700 text-yellow-100' };
		}
	}

	function getTaskName(index: number): string {
		const defaultTaskNames = ['视频封面', '视频信息', 'UP主头像', 'UP主信息', '分页下载'];
		return defaultTaskNames[index] || `任务${index + 1}`;
	}

	$: overallStatus = getOverallStatus(video.download_status, video.should_download, video.valid);
	$: completed = video.download_status.filter((status) => status === 7).length;
	$: total = video.download_status.length;

	async function handleReset() {
		resetting = true;
		if (onReset) {
			await onReset(forceReset);
		}
		resetting = false;
		resetDialogOpen = false;
		forceReset = false;
	}

	async function handleClearAndReset() {
		clearAndResetting = true;
		if (onClearAndReset) {
			await onClearAndReset();
		}
		clearAndResetting = false;
		clearAndResetDialogOpen = false;
	}
</script>

<div
	class="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3 gap-y-2 px-4 py-3 transition-colors hover:bg-muted/40 md:grid-cols-[minmax(0,1fr)_9rem_10rem_5rem_8rem] md:gap-6"
>
	<!-- 标题 / UP 主（批量选择时左侧带勾选框） -->
	<div class="col-span-2 flex min-w-0 items-start gap-2.5 md:col-span-1 md:order-1">
		{#if selectable}
			<Checkbox
				class="mt-0.5 shrink-0"
				checked={selected}
				aria-label={`选择视频 ${video.name}`}
				onCheckedChange={(value: boolean) => onToggleSelect?.(value)}
			/>
		{/if}
		<div class="min-w-0 flex-1">
			<a
				href={`/video/${video.id}`}
				class="line-clamp-1 text-sm leading-relaxed font-medium hover:underline"
				title={video.name}
			>
				{video.name}
			</a>
			<div class="text-muted-foreground mt-1 flex min-w-0 items-center gap-1 text-xs">
				<UserIcon class="h-3 w-3 shrink-0" />
				<span class="min-w-0 truncate" title={video.upper_name}>{video.upper_name}</span>
			</div>
		</div>
	</div>

	<!-- 视频来源 -->
	<div class="min-w-0 md:order-2">
		{#if source}
			<Badge variant="outline" class="h-6 max-w-full px-1.5 text-xs font-normal">
				{#if source.type === 'favorite'}
					<HeartIcon class="h-3 w-3 shrink-0" />
				{:else if source.type === 'collection'}
					<FolderIcon class="h-3 w-3 shrink-0" />
				{:else if source.type === 'submission'}
					<UserIcon class="h-3 w-3 shrink-0" />
				{:else if source.type === 'watch_later'}
					<ClockIcon class="h-3 w-3 shrink-0" />
				{/if}
				<span class="min-w-0 truncate" title={source.name}>{source.name}</span>
			</Badge>
		{:else}
			<span class="text-muted-foreground/50 text-xs">-</span>
		{/if}
	</div>

	<!-- 整体状态（桌面端排在进度之后，见 md:order） -->
	<div class="justify-self-end md:order-4 md:justify-self-start">
		<Badge
			variant="secondary"
			class="px-2 py-1 text-xs font-medium {overallStatus.style}"
			title={overallStatus.text === '失败'
				? '存在失败的下载任务，可到「失败视频」页集中重试'
				: undefined}
		>
			{overallStatus.text}
		</Badge>
	</div>

	<!-- 下载进度：悬停进度条查看每个子任务 -->
	<div class="min-w-0 md:order-3">
		<Tooltip.Root>
			<Tooltip.Trigger class="w-full cursor-help">
				<div class="flex w-full items-center gap-2">
					<div class="flex w-full gap-0.5">
						{#each video.download_status as status, index (index)}
							<div class="h-1.5 flex-1 rounded-full {getSegmentColor(status)}"></div>
						{/each}
					</div>
					<span class="text-muted-foreground shrink-0 text-xs tabular-nums">
						{completed}/{total}
					</span>
				</div>
			</Tooltip.Trigger>
			<Tooltip.Content class="p-2">
				<div class="space-y-0.5 text-xs">
					{#each video.download_status as status, index (index)}
						<div>{getTaskName(index)}：{getStatusText(status)}</div>
					{/each}
				</div>
			</Tooltip.Content>
		</Tooltip.Root>
	</div>

	<!-- 操作 -->
	<div class="flex items-center justify-end gap-2 justify-self-end md:order-5">
		<Button
			size="sm"
			variant="outline"
			class="hover:bg-accent hover:text-accent-foreground h-8 shrink-0 cursor-pointer px-2.5 text-xs font-medium"
			onclick={() => goto(`/video/${video.id}`)}
		>
			<InfoIcon class="mr-1 h-3 w-3 shrink-0" />
			详情
		</Button>

		<DropdownMenu.Root>
			<DropdownMenu.Trigger>
				{#snippet child({ props })}
					<Button
						{...props}
						size="sm"
						variant="outline"
						class="hover:bg-accent hover:text-accent-foreground h-8 shrink-0 cursor-pointer px-2"
					>
						<EllipsisIcon class="h-3 w-3" />
					</Button>
				{/snippet}
			</DropdownMenu.Trigger>
			<DropdownMenu.Content align="end" class="w-48">
				<DropdownMenu.Item class="cursor-pointer" onclick={() => (resetDialogOpen = true)}>
					<RotateCcwIcon class="mr-2 h-4 w-4" />
					重置
				</DropdownMenu.Item>
				<DropdownMenu.Item class="cursor-pointer" onclick={() => (clearAndResetDialogOpen = true)}>
					<BrushCleaningIcon class="mr-2 h-4 w-4" />
					清空重置
				</DropdownMenu.Item>
				<DropdownMenu.Item
					class="cursor-pointer"
					onclick={() => window.open(`https://www.bilibili.com/video/${video.bvid}/`, '_blank')}
				>
					<SquareArrowOutUpRightIcon class="mr-2 h-4 w-4" />
					在 B 站打开
				</DropdownMenu.Item>
			</DropdownMenu.Content>
		</DropdownMenu.Root>
	</div>
</div>

<!-- 重置确认对话框 -->
<AlertDialog.Root bind:open={resetDialogOpen}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>重置视频</AlertDialog.Title>
			<AlertDialog.Description>
				确定要重置视频 <strong>"{video.name}"</strong> 的下载状态吗？
				<br />
				此操作会将所有的失败状态重置为未开始，<span class="text-destructive font-medium"
					>无法撤销</span
				>。
			</AlertDialog.Description>
		</AlertDialog.Header>

		<div class="space-y-4 py-4">
			<div class="rounded-lg border border-orange-200 bg-orange-50 p-3">
				<div class="mb-2 flex items-center space-x-2">
					<Checkbox id={`force-reset-${video.id}`} bind:checked={forceReset} />
					<Label for={`force-reset-${video.id}`} class="text-sm font-medium text-orange-700"
						>⚠️ 强制重置</Label
					>
				</div>
				<p class="text-xs leading-relaxed text-orange-700">
					除重置失败状态外还会检查修复任务状态的标识位 <br />
					版本升级引入新任务时勾选该选项进行重置，可以允许旧视频执行新任务
				</p>
			</div>
		</div>

		<AlertDialog.Footer>
			<AlertDialog.Cancel
				onclick={() => {
					forceReset = false;
				}}>取消</AlertDialog.Cancel
			>
			<AlertDialog.Action
				onclick={handleReset}
				disabled={resetting}
				class={forceReset ? 'bg-orange-600 hover:bg-orange-700' : ''}
			>
				{resetting ? '重置中...' : forceReset ? '确认强制重置' : '确认重置'}
			</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>

<!-- 清空重置确认对话框 -->
<AlertDialog.Root bind:open={clearAndResetDialogOpen}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>清空重置视频</AlertDialog.Title>
			<AlertDialog.Description>
				确定要清空重置视频 <strong>"{video.name}"</strong> 吗？
				<br />
				<br />
				此操作会：
				<ul class="mt-2 ml-4 list-disc space-y-1">
					<li>将视频状态重置为未开始</li>
					<li>删除所有分页信息</li>
					<li class="text-destructive font-medium">删除视频对应的文件夹</li>
				</ul>
				<br />
				该功能可在多页视频变更后手动触发全量更新，执行后<span class="text-destructive font-medium"
					>无法撤销</span
				>。
			</AlertDialog.Description>
		</AlertDialog.Header>

		<AlertDialog.Footer>
			<AlertDialog.Cancel>取消</AlertDialog.Cancel>
			<AlertDialog.Action
				onclick={handleClearAndReset}
				disabled={clearAndResetting}
				class="bg-destructive hover:bg-destructive/90"
			>
				{clearAndResetting ? '清空重置中...' : '确认清空重置'}
			</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>
