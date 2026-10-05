"GameInfo"
{
	game 		"citadel"
	title 		"Citadel"

	FileSystem
	{
		SearchPaths
		{
			Game				citadel
			Game				core
		}
	}

	RenderSystem
	{
		VulkanUseSecondaryCommandBuffers 1
	}

	SceneSystem
	{
		GpuLightBinner 1
		CSMCascadeResolution 0
	}

	ConVars
	{	
		r_citadel_selection_outline2_alpha "255"
		r_citadel_selection_outline2_fade_pow -inf
		citadel_player_outline_fade_range_min inf

		"r_ssao" "false"
		"r_shadows" "0"
		"cl_ragdoll_limit" "0"

		"voice_always_sample_mic"               
		{
			"version" "2"
			"default"	"0"
		}

		"citadel_damage_indicator_radius" "1"
	}
}
