<script lang="ts">
	import { icons, type IconName } from "./icons";
	let { name }: { name: IconName } = $props();
</script>

<img
	class="icon"
	src={`/icons/${name}.svg`}
	width={icons[name][0]}
	height={icons[name][1]}
	alt=""
	aria-hidden="true"
/>
