<script lang="ts">
	import VideoListRow from '$lib/components/video-list-row.svelte';
	import { Button } from '$lib/components/ui/button/index.js';
	import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js';
	import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
	import SquarePenIcon from '@lucide/svelte/icons/square-pen';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import DatabaseIcon from '@lucide/svelte/icons/database';
	import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
	import SlidersHorizontalIcon from '@lucide/svelte/icons/sliders-horizontal';
	import XIcon from '@lucide/svelte/icons/x';
	import api from '$lib/api';
	import { Checkbox } from '$lib/components/ui/checkbox/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Switch } from '$lib/components/ui/switch/index.js';
	import type {
		VideosResponse,
		VideoSourcesResponse,
		ApiError,
		VideoSource,
		TaskStatus,
		UpdateFilteredVideoStatusRequest,
		VideoInfo
	} from '$lib/types';
	import { onDestroy, onMount } from 'svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { VIDEO_SOURCES } from '$lib/consts';
	import { setBreadcrumb } from '$lib/stores/breadcrumb';
	import {
		appStateStore,
		setAll,
		setCreatedTimeFilter,
		setQuery,
		setStatusFilter,
		setValidationFilter,
		ToQuery,
		ToFilterParams,
		hasActiveFilters,
		type AppState,
		type StatusFilterValue,
		type ValidationFilterValue
	} from '$lib/stores/filter';
	import { toast } from 'svelte-sonner';
	import DropdownFilter, { type Filter } from '$lib/components/dropdown-filter.svelte';
	import SearchBar from '$lib/components/search-bar.svelte';
	import FilteredStatusEditor from '$lib/components/filtered-status-editor.svelte';
	import StatusFilter from '$lib/components/status-filter.svelte';
	import ValidationFilter from '$lib/components/validation-filter.svelte';
	import CreatedTimeFilter from '$lib/components/created-time-filter.svelte';
	import { SvelteMap } from 'svelte/reactivity';

	// 一次加载全部视频：后端只有在 page 与 page_size 同时存在时才按分页返回，
	// 因此固定 page=0 并给一个足够大的 page_size，等价于不分页。
	const FETCH_ALL_PAGE_SIZE = 1000000;

	let videosData: VideosResponse | null = null;
	let loading = false;

	// 筛选面板、自动刷新与实时任务状态
	let filterPanelOpen = false;
	let autoRefresh = true;
	let taskStatus: TaskStatus | null = null;
	let lastRefreshAt: Date | null = null;
	let unsubscribeTasks: (() => void) | null = null;
	let refreshTimer: ReturnType<typeof setInterval> | null = null;

	let lastSearch: string | null = null;

	let resetAllDialogOpen = false;
	let resettingAll = false;

	let forceReset = false;

	let updateAllDialogOpen = false;
	let updatingAll = false;

	// 列表批量选择：勾选后直接对选中的视频操作，与筛选条件互不影响
	let selectedIds: number[] = [];
	let batchResetDialogOpen = false;
	let batchResetForce = false;
	let batchResetting = false;
	let batchUpdateDialogOpen = false;
	let batchUpdating = false;

	let videoSources: VideoSourcesResponse | null = null;
	let videoSourcesLoaded = false;
	let filters: Record<string, Filter> | null = null;
	let sourceMap: SvelteMap<string, { type: string; name: string }> = new SvelteMap();

	function getApiParams(searchParams: URLSearchParams) {
		let videoSource = null;
		for (const source of Object.values(VIDEO_SOURCES)) {
			const value = searchParams.get(source.type);
			if (value) {
				videoSource = { type: source.type, id: value };
			}
		}
		// 支持从 URL 里还原状态筛选
		const statusFilterParam = searchParams.get('status_filter');
		const statusFilter: StatusFilterValue | null =
			statusFilterParam === 'failed' ||
			statusFilterParam === 'succeeded' ||
			statusFilterParam === 'waiting'
				? statusFilterParam
				: null;
		const validationFilterParam = searchParams.get('validation_filter');
		const validationFilter: ValidationFilterValue =
			validationFilterParam === 'skipped' ||
			validationFilterParam === 'invalid' ||
			validationFilterParam === 'normal'
				? validationFilterParam
				: null;
		return {
			query: searchParams.get('query') || '',
			videoSource,
			statusFilter,
			validationFilter,
			createdFrom: searchParams.get('created_from'),
			createdTo: searchParams.get('created_to')
		};
	}

	async function loadVideos(
		query: string,
		filter?: { type: string; id: string } | null,
		statusFilter: StatusFilterValue | null = null,
		validationFilter: ValidationFilterValue | null = null,
		createdFrom: string | null = null,
		createdTo: string | null = null
	) {
		loading = true;
		try {
			const params: Record<string, string | number | boolean> = {
				page: 0,
				page_size: FETCH_ALL_PAGE_SIZE
			};
			if (query) {
				params.query = query;
			}
			if (filter) {
				params[filter.type] = parseInt(filter.id);
			}
			if (statusFilter) {
				params.status_filter = statusFilter;
			}
			if (validationFilter) {
				params.validation_filter = validationFilter;
			}
			if (createdFrom) {
				params.created_from = createdFrom;
			}
			if (createdTo) {
				params.created_to = createdTo;
			}
			const result = await api.getVideos(params);
			videosData = result.data;
			lastRefreshAt = new Date();
		} catch (error) {
			console.error('加载视频失败：', error);
			toast.error('加载视频失败', {
				description: (error as ApiError).message
			});
		} finally {
			loading = false;
		}
	}

	async function reloadVideos() {
		const { query, videoSource, statusFilter, validationFilter, createdFrom, createdTo } =
			$appStateStore;
		await loadVideos(query, videoSource, statusFilter, validationFilter, createdFrom, createdTo);
	}

	async function handleSearchParamsChange(searchParams: URLSearchParams) {
		const { query, videoSource, statusFilter, validationFilter, createdFrom, createdTo } =
			getApiParams(searchParams);
		clearSelection();
		setAll(query, videoSource, statusFilter, validationFilter, createdFrom, createdTo);
		loadVideos(query, videoSource, statusFilter, validationFilter, createdFrom, createdTo);
	}

	type FilterChipKey = 'query' | 'source' | 'status' | 'validation' | 'created';

	interface FilterChip {
		key: FilterChipKey;
		label: string;
	}

	// 已生效的筛选条件，展示为可单独移除的 chips
	function getFilterChips(state: AppState): FilterChip[] {
		const chips: FilterChip[] = [];
		if (state.query.trim()) {
			chips.push({ key: 'query', label: `搜索：${state.query}` });
		}
		if (state.videoSource) {
			const typeTitle =
				Object.values(VIDEO_SOURCES).find((s) => s.type === state.videoSource?.type)?.title ??
				'来源';
			const sourceName =
				sourceMap.get(`${state.videoSource.type}:${state.videoSource.id}`)?.name ??
				state.videoSource.id;
			chips.push({ key: 'source', label: `${typeTitle}：${sourceName}` });
		}
		if (state.statusFilter) {
			const labels = { failed: '仅失败', succeeded: '仅成功', waiting: '仅等待' };
			chips.push({ key: 'status', label: `状态：${labels[state.statusFilter]}` });
		}
		if (state.validationFilter) {
			const labels = { skipped: '跳过', invalid: '失效', normal: '有效' };
			chips.push({ key: 'validation', label: `有效性：${labels[state.validationFilter]}` });
		}
		if (state.createdFrom || state.createdTo) {
			const createdFrom = state.createdFrom?.replace('T', ' ') || '不限';
			const createdTo = state.createdTo?.replace('T', ' ') || '不限';
			chips.push({ key: 'created', label: `创建时间：${createdFrom} 至 ${createdTo}` });
		}
		return chips;
	}

	function applyStateAndGo(mutate: () => void) {
		mutate();
		goto(`/${ToQuery($appStateStore)}`);
	}

	function removeFilter(key: FilterChipKey) {
		const state = $appStateStore;
		switch (key) {
			case 'query':
				applyStateAndGo(() => setQuery(''));
				break;
			case 'source':
				applyStateAndGo(() =>
					setAll(
						state.query,
						null,
						state.statusFilter,
						state.validationFilter,
						state.createdFrom,
						state.createdTo
					)
				);
				break;
			case 'status':
				applyStateAndGo(() => setStatusFilter(null));
				break;
			case 'validation':
				applyStateAndGo(() => setValidationFilter(null));
				break;
			case 'created':
				applyStateAndGo(() => setCreatedTimeFilter(null, null));
				break;
		}
	}

	function clearAllFilters() {
		filterPanelOpen = false;
		applyStateAndGo(() => setAll('', null, null, null, null, null));
	}

	function toggleSelect(id: number, checked: boolean) {
		if (checked) {
			if (!selectedIds.includes(id)) {
				selectedIds = [...selectedIds, id];
			}
		} else {
			selectedIds = selectedIds.filter((selectedId) => selectedId !== id);
		}
	}

	function clearSelection() {
		if (selectedIds.length > 0) {
			selectedIds = [];
		}
	}

	function toggleSelectAll(checked: boolean) {
		selectedIds = checked && videosData ? videosData.videos.map((video) => video.id) : [];
	}

	async function handleBatchReset() {
		if (selectedIds.length === 0) return;
		batchResetting = true;
		try {
			const result = await api.resetFilteredVideoStatus({
				ids: [...selectedIds],
				force: batchResetForce
			});
			const data = result.data;
			if (data.resetted) {
				toast.success('重置成功', {
					description: `已重置 ${data.resetted_videos_count} 个视频和 ${data.resetted_pages_count} 个分页`
				});
				clearSelection();
				await reloadVideos();
			} else {
				toast.info('没有需要重置的视频');
			}
		} catch (error) {
			console.error('批量重置失败：', error);
			toast.error('批量重置失败', {
				description: (error as ApiError).message
			});
		} finally {
			batchResetting = false;
			batchResetDialogOpen = false;
			batchResetForce = false;
		}
	}

	async function handleBatchUpdate(request: UpdateFilteredVideoStatusRequest) {
		if (selectedIds.length === 0) return;
		batchUpdating = true;
		try {
			const result = await api.updateFilteredVideoStatus({
				...request,
				ids: [...selectedIds]
			});
			const data = result.data;
			if (data.success) {
				toast.success('更新成功', {
					description: `已更新 ${data.updated_videos_count} 个视频和 ${data.updated_pages_count} 个分页`
				});
				clearSelection();
				await reloadVideos();
			} else {
				toast.info('没有视频被更新');
			}
		} catch (error) {
			console.error('批量更新失败：', error);
			toast.error('批量更新失败', {
				description: (error as ApiError).message
			});
		} finally {
			batchUpdating = false;
			batchUpdateDialogOpen = false;
		}
	}

	$: allSelected =
		!!videosData && videosData.videos.length > 0 && selectedIds.length === videosData.videos.length;
	$: someSelected = selectedIds.length > 0 && !allSelected;

	function startPolling() {
		stopPolling();
		refreshTimer = setInterval(() => {
			// 页面在后台时不刷新，避免无谓请求
			if (autoRefresh && !document.hidden) {
				reloadVideos();
			}
		}, 30000);
	}

	function stopPolling() {
		if (refreshTimer) {
			clearInterval(refreshTimer);
			refreshTimer = null;
		}
	}

	function setAutoRefresh(value: boolean) {
		autoRefresh = value;
		localStorage.setItem('videos-auto-refresh', String(value));
		if (value) {
			startPolling();
		} else {
			stopPolling();
		}
	}

	async function handleResetVideo(id: number, forceReset: boolean) {
		try {
			const result = await api.resetVideoStatus(id, { force: forceReset });
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

	async function handleResetAllVideos() {
		resettingAll = true;
		try {
			// 获取筛选参数
			const filterParams = ToFilterParams($appStateStore);
			const result = await api.resetFilteredVideoStatus({
				...filterParams,
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

	async function handleUpdateAllVideos(request: UpdateFilteredVideoStatusRequest) {
		updatingAll = true;
		try {
			// 获取筛选参数并合并
			const filterParams = ToFilterParams($appStateStore);
			const fullRequest = {
				...filterParams,
				...request
			};
			const result = await api.updateFilteredVideoStatus(fullRequest);
			const data = result.data;
			if (data.success) {
				toast.success('更新成功', {
					description: `已更新 ${data.updated_videos_count} 个视频和 ${data.updated_pages_count} 个分页`
				});
				await reloadVideos();
			} else {
				toast.info('没有视频被更新');
			}
		} catch (error) {
			console.error('更新失败：', error);
			toast.error('更新失败', {
				description: (error as ApiError).message
			});
		} finally {
			updatingAll = false;
			updateAllDialogOpen = false;
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

	// 获取筛选条件的显示数组
	function getFilterDescriptionParts(): string[] {
		const state = $appStateStore;
		const parts: string[] = [];
		if (state.query.trim()) {
			parts.push(`搜索词："${state.query}"`);
		}
		if (state.videoSource && videoSources) {
			const sourceType = state.videoSource.type;
			const sourceId = parseInt(state.videoSource.id);
			const sourceConfig = Object.values(VIDEO_SOURCES).find((s) => s.type === sourceType);
			if (sourceConfig) {
				const sourceList = videoSources[sourceType as keyof VideoSourcesResponse] as VideoSource[];
				const source = sourceList.find((s) => s.id === sourceId);
				if (source) {
					parts.push(`${sourceConfig.title}：${source.name}`);
				}
			}
		}
		if (state.statusFilter) {
			const statusLabels = {
				failed: '仅失败',
				succeeded: '仅成功',
				waiting: '仅等待'
			};
			parts.push(`状态：${statusLabels[state.statusFilter]}`);
		}
		if (state.validationFilter) {
			const validationLabels = {
				skipped: '跳过',
				invalid: '失效',
				normal: '有效'
			};
			parts.push(`有效性：${validationLabels[state.validationFilter]}`);
		}
		if (state.createdFrom || state.createdTo) {
			const createdFrom = state.createdFrom?.replace('T', ' ') || '不限';
			const createdTo = state.createdTo?.replace('T', ' ') || '不限';
			parts.push(`创建时间：${createdFrom} 至 ${createdTo}`);
		}
		return parts;
	}

	$: if (videoSourcesLoaded && $page.url.search !== lastSearch) {
		lastSearch = $page.url.search;
		handleSearchParamsChange($page.url.searchParams);
	}

	$: if (videoSources) {
		filters = Object.fromEntries(
			Object.values(VIDEO_SOURCES).map((source) => [
				source.type,
				{
					name: source.title,
					icon: source.icon,
					values: Object.fromEntries(
						(videoSources![source.type as keyof VideoSourcesResponse] as VideoSource[]).map(
							(item) => [item.id, item.name]
						)
					)
				}
			])
		);
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
		filters = null;
		sourceMap.clear();
	}

	onMount(async () => {
		setBreadcrumb([
			{
				label: '视频'
			}
		]);
		const savedAutoRefresh = localStorage.getItem('videos-auto-refresh');
		if (savedAutoRefresh !== null) {
			autoRefresh = savedAutoRefresh === 'true';
		}
		if (autoRefresh) {
			startPolling();
		}
		// 任务开始或结束时主动刷新，让下载进度尽量实时
		unsubscribeTasks = api.subscribeToTasks((data) => {
			const wasRunning = taskStatus?.is_running ?? false;
			taskStatus = data;
			if (autoRefresh && (data.is_running || wasRunning)) {
				reloadVideos();
			}
		});
		videoSources = (await api.getVideoSources()).data;
		videoSourcesLoaded = true;
	});

	onDestroy(() => {
		if (unsubscribeTasks) {
			unsubscribeTasks();
			unsubscribeTasks = null;
		}
		stopPolling();
	});

	$: filterChips = getFilterChips($appStateStore);
	$: hasFilters = hasActiveFilters($appStateStore);
	$: filterDescriptionParts = videoSources && $appStateStore && getFilterDescriptionParts();
</script>

<svelte:head>
	<title>视频 - Bili Sync</title>
</svelte:head>

<div class="mb-4 flex flex-col gap-3">
	<div class="flex flex-wrap items-center gap-2">
		<SearchBar
			placeholder="搜索视频标题或 BV 号.."
			widthClass="w-full max-w-xs"
			value={$appStateStore.query}
			onSearch={(value) => {
				setQuery(value);
				goto(`/${ToQuery($appStateStore)}`);
			}}
		></SearchBar>
		<Button
			size="sm"
			variant="outline"
			class="h-8 cursor-pointer text-xs font-medium"
			onclick={() => (filterPanelOpen = !filterPanelOpen)}
		>
			<SlidersHorizontalIcon class="mr-1.5 h-3.5 w-3.5" />
			筛选
			{#if filterChips.length > 0}
				<Badge class="ml-1 h-4 rounded px-1 text-[10px]">{filterChips.length}</Badge>
			{/if}
			<ChevronDownIcon
				class="ml-0.5 h-3 w-3 transition-transform {filterPanelOpen ? 'rotate-180' : ''}"
			/>
		</Button>
		{#if taskStatus?.is_running}
			<Badge variant="secondary" class="h-6 gap-1.5 text-xs font-normal">
				<span class="h-1.5 w-1.5 animate-pulse rounded-full bg-emerald-500"></span>
				任务运行中
			</Badge>
		{/if}
		<div class="ml-auto flex items-center gap-3">
			<label
				class="text-muted-foreground flex cursor-pointer items-center gap-1.5 text-xs select-none"
			>
				<Switch
					checked={autoRefresh}
					onCheckedChange={(value: boolean) => setAutoRefresh(value)}
					class="scale-75"
				/>
				自动刷新
			</label>
			{#if lastRefreshAt}
				<span class="text-muted-foreground/70 text-xs">
					{lastRefreshAt.toLocaleTimeString('zh-CN')} 更新
				</span>
			{/if}
			<Button
				size="sm"
				variant="outline"
				class="h-8 w-8 px-0"
				title="刷新"
				onclick={() => reloadVideos()}
				disabled={loading}
			>
				<RefreshCwIcon class="h-3.5 w-3.5 {loading ? 'animate-spin' : ''}" />
			</Button>
		</div>
	</div>

	{#if filterPanelOpen}
		<div class="grid gap-4 rounded-lg border bg-muted/30 p-4 sm:grid-cols-2 lg:grid-cols-4">
			<div class="space-y-2">
				<div class="text-muted-foreground text-xs font-medium">创建时间</div>
				<CreatedTimeFilter
					createdFrom={$appStateStore.createdFrom}
					createdTo={$appStateStore.createdTo}
					onChange={(createdFrom, createdTo) => {
						setCreatedTimeFilter(createdFrom, createdTo);
						goto(`/${ToQuery($appStateStore)}`);
					}}
				/>
			</div>
			<div class="space-y-2">
				<div class="text-muted-foreground text-xs font-medium">有效性</div>
				<ValidationFilter
					value={$appStateStore.validationFilter}
					onSelect={(value) => {
						setValidationFilter(value);
						goto(`/${ToQuery($appStateStore)}`);
					}}
					onRemove={() => {
						setValidationFilter(null);
						goto(`/${ToQuery($appStateStore)}`);
					}}
				/>
			</div>
			<div class="space-y-2">
				<div class="text-muted-foreground text-xs font-medium">状态</div>
				<StatusFilter
					value={$appStateStore.statusFilter}
					onSelect={(value) => {
						setStatusFilter(value);
						goto(`/${ToQuery($appStateStore)}`);
					}}
					onRemove={() => {
						setStatusFilter(null);
						goto(`/${ToQuery($appStateStore)}`);
					}}
				/>
			</div>
			<div class="space-y-2">
				<div class="text-muted-foreground text-xs font-medium">来源</div>
				<DropdownFilter
					{filters}
					selectedLabel={$appStateStore.videoSource}
					onSelect={(type, id) => {
						setAll(
							'',
							{ type, id },
							$appStateStore.statusFilter,
							$appStateStore.validationFilter,
							$appStateStore.createdFrom,
							$appStateStore.createdTo
						);
						goto(`/${ToQuery($appStateStore)}`);
					}}
					onRemove={() => {
						setAll(
							'',
							null,
							$appStateStore.statusFilter,
							$appStateStore.validationFilter,
							$appStateStore.createdFrom,
							$appStateStore.createdTo
						);
						goto(`/${ToQuery($appStateStore)}`);
					}}
				/>
			</div>
		</div>
	{/if}

	{#if filterChips.length > 0}
		<div class="flex flex-wrap items-center gap-2">
			{#each filterChips as chip (chip.key)}
				<Badge variant="secondary" class="gap-1 py-1 pr-1 pl-2 text-xs font-normal">
					{chip.label}
					<button
						class="hover:bg-background/80 cursor-pointer rounded p-0.5"
						aria-label={`移除筛选 ${chip.label}`}
						onclick={() => removeFilter(chip.key)}
					>
						<XIcon class="h-3 w-3" />
					</button>
				</Badge>
			{/each}
			<Button variant="ghost" size="sm" class="h-6 px-2 text-xs" onclick={clearAllFilters}>
				清除全部
			</Button>
		</div>
	{/if}
</div>

{#if videosData}
	<div class="mb-6 flex items-center justify-between">
		<div class="flex items-center gap-6">
			<div class="text-sm font-medium">共 {videosData.total_count} 个视频</div>
		</div>
		<div class="flex items-center gap-2">
			<Button
				size="sm"
				variant="outline"
				class="hover:bg-accent hover:text-accent-foreground h-8 cursor-pointer text-xs font-medium"
				onclick={() => (updateAllDialogOpen = true)}
				disabled={updatingAll || loading}
			>
				<SquarePenIcon class="mr-1.5 h-3 w-3" />
				{hasFilters ? '编辑筛选' : '编辑全部'}
			</Button>
			<Button
				size="sm"
				variant="outline"
				class="hover:bg-accent hover:text-accent-foreground h-8 cursor-pointer text-xs font-medium"
				onclick={() => (resetAllDialogOpen = true)}
				disabled={resettingAll || loading}
			>
				<RotateCcwIcon class="mr-1.5 h-3 w-3 {resettingAll ? 'animate-spin' : ''}" />
				{hasFilters ? '重置筛选' : '重置全部'}
			</Button>
		</div>
	</div>
{/if}

{#if selectedIds.length > 0}
	<div class="bg-muted/40 mb-4 flex flex-wrap items-center gap-3 rounded-lg border px-4 py-2.5">
		<span class="text-sm font-medium">已选择 {selectedIds.length} 个视频</span>
		<div class="ml-auto flex items-center gap-2">
			<Button
				size="sm"
				variant="outline"
				class="h-8 cursor-pointer text-xs font-medium"
				onclick={() => (batchUpdateDialogOpen = true)}
				disabled={batchUpdating}
			>
				<SquarePenIcon class="mr-1.5 h-3 w-3" />
				编辑状态
			</Button>
			<Button
				size="sm"
				variant="outline"
				class="h-8 cursor-pointer text-xs font-medium"
				onclick={() => (batchResetDialogOpen = true)}
				disabled={batchResetting}
			>
				<RotateCcwIcon class="mr-1.5 h-3 w-3 {batchResetting ? 'animate-spin' : ''}" />
				重置
			</Button>
			<Button size="sm" variant="ghost" class="h-8 text-xs" onclick={clearSelection}>
				取消选择
			</Button>
		</div>
	</div>
{/if}

{#if loading && !videosData}
	<div class="flex items-center justify-center py-16">
		<div class="text-muted-foreground/70 text-sm">加载中...</div>
	</div>
{:else if videosData?.videos.length}
	<div class="mb-8 overflow-hidden rounded-lg border">
		<!-- 表头只在桌面端显示，与列表行共用同一套网格列宽 -->
		<div
			class="text-muted-foreground bg-muted/40 hidden items-center gap-6 border-b px-4 py-2 text-xs font-medium md:grid md:grid-cols-[minmax(0,1fr)_9rem_10rem_5rem_8rem]"
		>
			<div class="flex items-center gap-2.5">
				<Checkbox
					checked={allSelected}
					indeterminate={someSelected}
					onCheckedChange={(value: boolean) => toggleSelectAll(value)}
					aria-label="全选当前列表"
				/>
				<span>视频</span>
			</div>
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
					selectable
					selected={selectedIds.includes(video.id)}
					onToggleSelect={(checked: boolean) => toggleSelect(video.id, checked)}
					onReset={async (forceReset: boolean) => {
						await handleResetVideo(video.id, forceReset);
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
			<div class="space-y-1.5">
				<p class="text-muted-foreground text-sm">
					{hasFilters ? '没有符合筛选条件的视频' : '暂无视频数据'}
				</p>
				<p class="text-muted-foreground/70 text-xs">
					{hasFilters
						? '可以调整或清除筛选条件后重试'
						: '先在「视频源」里添加要同步的收藏夹、合集或 UP 主'}
				</p>
			</div>
			{#if hasFilters}
				<Button variant="outline" size="sm" onclick={clearAllFilters}>清除筛选条件</Button>
			{:else}
				<Button href="/video-sources" variant="outline" size="sm">
					<DatabaseIcon class="mr-1.5 h-3.5 w-3.5" />
					去添加视频源
				</Button>
			{/if}
		</div>
	</div>
{/if}

<AlertDialog.Root bind:open={resetAllDialogOpen}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>{hasFilters ? '重置筛选视频' : '重置全部视频'}</AlertDialog.Title>
			<AlertDialog.Description>
				{#if hasFilters}
					确定要重置<strong>符合以下筛选条件</strong>的视频的下载状态吗？<br />
					<div class="bg-muted my-2 rounded-md p-2 text-left">
						{#each filterDescriptionParts as part, index (index)}
							<div><strong>{part}</strong></div>
						{/each}
					</div>
				{:else}
					确定要重置<strong>全部视频</strong>的下载状态吗？<br />
				{/if}
				此操作会将所有的失败状态重置为未开始，<span class="text-destructive font-medium"
					>无法撤销</span
				>。
			</AlertDialog.Description>
		</AlertDialog.Header>

		<div class="py-2">
			<div class="rounded-lg border border-orange-200 bg-orange-50 p-3">
				<div class="mb-2 flex items-center space-x-2">
					<Checkbox id="force-reset-all" bind:checked={forceReset} />
					<Label for="force-reset-all" class="text-sm font-medium text-orange-700"
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
				onclick={handleResetAllVideos}
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

<FilteredStatusEditor
	bind:open={updateAllDialogOpen}
	{hasFilters}
	loading={updatingAll}
	filterDescriptionParts={filterDescriptionParts || []}
	onsubmit={handleUpdateAllVideos}
/>

<AlertDialog.Root bind:open={batchResetDialogOpen}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>重置选中的视频</AlertDialog.Title>
			<AlertDialog.Description>
				确定要重置<strong>已选中的 {selectedIds.length} 个视频</strong>的下载状态吗？
				<br />
				此操作会将所有的失败状态重置为未开始，<span class="text-destructive font-medium"
					>无法撤销</span
				>。
			</AlertDialog.Description>
		</AlertDialog.Header>

		<div class="py-2">
			<div class="rounded-lg border border-orange-200 bg-orange-50 p-3">
				<div class="mb-2 flex items-center space-x-2">
					<Checkbox id="force-reset-batch" bind:checked={batchResetForce} />
					<Label for="force-reset-batch" class="text-sm font-medium text-orange-700"
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
			<AlertDialog.Cancel disabled={batchResetting}>取消</AlertDialog.Cancel>
			<AlertDialog.Action
				onclick={handleBatchReset}
				disabled={batchResetting}
				class={batchResetForce ? 'bg-orange-600 hover:bg-orange-700' : ''}
			>
				{#if batchResetting}
					<RotateCcwIcon class="mr-2 h-4 w-4 animate-spin" />
					重置中...
				{:else}
					{batchResetForce ? '确认强制重置' : '确认重置'}
				{/if}
			</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>

<FilteredStatusEditor
	bind:open={batchUpdateDialogOpen}
	titleOverride="编辑选中视频"
	descriptionOverride={`正在编辑已选中的 ${selectedIds.length} 个视频的下载状态。`}
	loading={batchUpdating}
	onsubmit={handleBatchUpdate}
/>
