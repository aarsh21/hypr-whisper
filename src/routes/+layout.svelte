<script lang="ts">
	import './layout.css';
	import { Toaster } from "$lib/components/ui/sonner";
	import { ModeWatcher } from "mode-watcher";
	
	let { children } = $props();
</script>

<ModeWatcher defaultMode="dark" />
<Toaster richColors position="bottom-center" />
{@render children()}
