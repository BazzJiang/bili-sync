<script lang="ts">
	import { onMount } from 'svelte';
	import api from '$lib/api';
	import { toast } from 'svelte-sonner';
	import { SvelteMap } from 'svelte/reactivity';
	import VideoListRow from '$lib/components/video-list-row.svelte';
	import { Button } from '$lib/components/ui/button/index.js';
	import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js';
	import { Checkbox } from '$lib/components/ui/checkbox/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import CircleCheckBigIcon from '@lucide/svelte/icons/circle-check-big';
	import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
	import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
	import ArrowLeftIcon from '@lucide/svelte/icons/arrow-left';
	import { VIDEO_SOURCES } from '$lib/consts';
	import { setBreadcrumb } from '$lib/stores/breadcrumb';
	import type {
		ApiError,
		VideoInfo,
		VideoSource,
		VideoSourcesResponse,
		VideosResponse
	} from '$lib/types';

	// 一次加载全部失败视频：后端只有在 page 与 page_size 同时存在时才按分页返回，
	// 因此固定 page=0 并给一个足够大的 page_size，等价于不分页。
	const FETCH_ALL_PAGE_SIZE = 1000000;

	let videosData: VideosResponse | null = null;
	let loading = false;

	let videoSources: VideoSourcesResponse | null = null;
	let sourceMap: SvelteMap<string, { type: string; name: string }> = new SvelteMap();

	let resetAllDialogOpen = false;
	let resettingAll = false;
	let forceReset = false;

	async function loadFailedVideos() {
		loading = true;
		try {
			const result = await api.getVideos({
				status_filter: 'failed',
				page: 0,
				page_size: FETCH_ALL_PAGE_SIZE
			});
			videosData = result.data;
		} catch (error) {
			console.error('加载失败视频失败：', error);
			toast.error('加载失败视频失败', {
				description: (error as ApiError).message
			});
		} finally {
			loading = false;
		}
	}

	async function loadVideoSources() {
		try {
			videoSources = (await api.getVideoSources()).data;
		} catch (error) {
			console.error('加载视频来源失败：', error);
			toast.error('加载视频来源失败', {
				description: (error as ApiError).message
			});
		}
	}

	// 来源名称只取决于视频源配置，刷新列表时无需重复拉取
	async function reloadVideos() {
		await loadFailedVideos();
	}

	async function handleResetVideo(id: number, force: boolean) {
		try {
			const result = await api.resetVideoStatus(id, { force });
			const data = result.data;
			if (data.resetted) {
				toast.success('重置成功', {
					description: `视频「${data.video.name}」已重置`
				});
				await reloadVideos();
			} else {
				toast.info('重置无效', {
					description: `视频「${data.video.name}」没有失败的状态，无需重置`
				});
			}
		} catch (error) {
			console.error('重置失败：', error);
			toast.error('重置失败', {
				description: (error as ApiError).message
			});
		}
	}

	async function handleClearAndResetVideo(id: number) {
		try {
			const result = await api.clearAndResetVideoStatus(id);
			const data = result.data;
			if (data.warning) {
				toast.warning('清空重置成功', {
					description: data.warning
				});
			} else {
				toast.success('清空重置成功', {
					description: `视频「${data.video.name}」已清空重置`
				});
			}
			await reloadVideos();
		} catch (error) {
			console.error('清空重置失败：', error);
			toast.error('清空重置失败', {
				description: (error as ApiError).message
			});
		}
	}

	async function handleResetAllFailedVideos() {
		resettingAll = true;
		try {
			const result = await api.resetFilteredVideoStatus({
				status_filter: 'failed',
				force: forceReset
			});
			const data = result.data;
			if (data.resetted) {
				toast.success('重置成功', {
					description: `已重置 ${data.resetted_videos_count} 个视频和 ${data.resetted_pages_count} 个分页`
				});
				await reloadVideos();
			} else {
				toast.info('没有需要重置的视频');
			}
		} catch (error) {
			console.error('重置失败：', error);
			toast.error('重置失败', {
				description: (error as ApiError).message
			});
		} finally {
			resettingAll = false;
			resetAllDialogOpen = false;
		}
	}

	function getVideoSource(video: VideoInfo): { type: string; name: string } | null {
		if (video.collection_id != null) {
			return sourceMap.get(`collection:${video.collection_id}`) || null;
		}
		if (video.favorite_id != null) {
			return sourceMap.get(`favorite:${video.favorite_id}`) || null;
		}
		if (video.submission_id != null) {
			return sourceMap.get(`submission:${video.submission_id}`) || null;
		}
		if (video.watch_later_id != null) {
			return sourceMap.get(`watch_later:${video.watch_later_id}`) || null;
		}
		return null;
	}

	$: if (videoSources) {
		sourceMap.clear();
		for (const source of Object.values(VIDEO_SOURCES)) {
			const sourceList = videoSources[source.type as keyof VideoSourcesResponse] as VideoSource[];
			for (const item of sourceList) {
				sourceMap.set(`${source.type}:${item.id}`, {
					type: source.type,
					name: item.name
				});
			}
		}
	} else {
		sourceMap.clear();
	}

	onMount(async () => {
		setBreadcrumb([{ label: '失败视频' }]);
		// 先加载来源映射再拉视频，避免首屏出现来源缺失的闪烁
		await loadVideoSources();
		await loadFailedVideos();
	});
</script>

<svelte:head>
	<title>失败视频 - Bili Sync</title>
</svelte:head>

<div class="mb-6 flex items-start justify-between gap-4">
	<div>
		<div class="text-sm font-medium">
			{#if videosData}
				共 {videosData.total_count} 个失败视频
			{:else}
				正在统计失败视频...
			{/if}
		</div>
		<p class="text-muted-foreground/70 mt-1 text-xs">
			下载失败的任务可以在这里集中重置，失败原因可在<a href="/logs" class="underline">日志页</a>查看
		</p>
	</div>
	<div class="flex shrink-0 items-center gap-2">
		<Button
			size="sm"
			variant="outline"
			class="hover:bg-accent hover:text-accent-foreground h-8 cursor-pointer text-xs font-medium"
			onclick={reloadVideos}
			disabled={loading}
		>
			<RefreshCwIcon class="mr-1.5 h-3 w-3 {loading ? 'animate-spin' : ''}" />
			刷新
		</Button>
		<Button
			size="sm"
			variant="destructive"
			class="h-8 cursor-pointer text-xs font-medium"
			onclick={() => (resetAllDialogOpen = true)}
			disabled={resettingAll || loading || !videosData?.videos.length}
		>
			<RotateCcwIcon class="mr-1.5 h-3 w-3 {resettingAll ? 'animate-spin' : ''}" />
			重置全部失败视频
		</Button>
	</div>
</div>

{#if loading}
	<div class="flex items-center justify-center py-16">
		<div class="text-muted-foreground/70 text-sm">加载中...</div>
	</div>
{:else if videosData?.videos.length}
	<div class="mb-8 overflow-hidden rounded-lg border">
		<!-- 表头只在桌面端显示，与列表行共用同一套网格列宽 -->
		<div
			class="text-muted-foreground bg-muted/40 hidden items-center gap-6 border-b px-4 py-2 text-xs font-medium md:grid md:grid-cols-[minmax(0,1fr)_9rem_10rem_5rem_8rem]"
		>
			<div>视频</div>
			<div>来源</div>
			<div>下载进度</div>
			<div>状态</div>
			<div class="text-right">操作</div>
		</div>
		<div class="divide-y">
			{#each videosData.videos as video (video.id)}
				<VideoListRow
					{video}
					source={getVideoSource(video)}
					onReset={async (force: boolean) => {
						await handleResetVideo(video.id, force);
					}}
					onClearAndReset={async () => {
						await handleClearAndResetVideo(video.id);
					}}
				/>
			{/each}
		</div>
	</div>
{:else}
	<div class="flex items-center justify-center py-16">
		<div class="space-y-4 text-center">
			<CircleCheckBigIcon class="text-muted-foreground/60 mx-auto h-12 w-12" />
			<div class="space-y-1">
				<p class="text-sm font-medium">暂无失败视频</p>
				<p class="text-muted-foreground/70 text-xs">所有视频的下载任务都正常，去视频列表看看吧</p>
			</div>
			<Button variant="outline" size="sm" href="/videos">
				<ArrowLeftIcon class="mr-1.5 h-3 w-3" />
				返回视频列表
			</Button>
		</div>
	</div>
{/if}

<AlertDialog.Root bind:open={resetAllDialogOpen}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>重置全部失败视频</AlertDialog.Title>
			<AlertDialog.Description>
				确定要重置<strong>全部 {videosData?.total_count ?? 0} 个失败视频</strong>的下载状态吗？<br
				/>
				此操作会将所有的失败状态重置为未开始，<span class="text-destructive font-medium"
					>无法撤销</span
				>。
			</AlertDialog.Description>
		</AlertDialog.Header>

		<div class="py-2">
			<div class="rounded-lg border border-orange-200 bg-orange-50 p-3">
				<div class="mb-2 flex items-center space-x-2">
					<Checkbox id="force-reset-all-failed" bind:checked={forceReset} />
					<Label for="force-reset-all-failed" class="text-sm font-medium text-orange-700"
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
				disabled={resettingAll}
				onclick={() => {
					forceReset = false;
				}}>取消</AlertDialog.Cancel
			>
			<AlertDialog.Action
				onclick={handleResetAllFailedVideos}
				disabled={resettingAll}
				class={forceReset ? 'bg-orange-600 hover:bg-orange-700' : ''}
			>
				{#if resettingAll}
					<RotateCcwIcon class="mr-2 h-4 w-4 animate-spin" />
					重置中...
				{:else}
					{forceReset ? '确认强制重置' : '确认重置'}
				{/if}
			</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>
