using DlFovFixer.Core.GameInfo;
using DlFovFixer.Core.Settings;

namespace DlFovFixer.Core.Applying;

/// <summary>
/// Applies the FOV value, and the extra tweaks when they are on, to gameinfo.gi and cfg/video.txt.
/// A file that already matches is not written, so applying on every tick is safe.
/// </summary>
public sealed class ApplyService(IGameFiles files)
{
    public ApplyOutcome Apply(AppSettings settings)
    {
        var path = settings.GameInfoPath;
        if (string.IsNullOrEmpty(path))
        {
            return new ApplyOutcome(ApplyResult.Missing);
        }

        GameInfoMergeOutcome? gameInfo = null;
        VideoConfigOutcome? video = null;
        try
        {
            var text = files.ReadText(path);
            if (text is null)
            {
                return new ApplyOutcome(ApplyResult.Missing);
            }

            var tweaks = settings.Tweaks;
            gameInfo = GameInfoMerge.Apply(text, settings.FovValue, tweaks.ConVars, tweaks.SceneSystem, settings.ApplyTweaks);
            if (gameInfo.Changed)
            {
                files.WriteText(path, gameInfo.Text);
            }

            if (settings.ApplyTweaks && tweaks.Video.Count > 0)
            {
                var videoPath = VideoConfigPathOf(path);
                video = VideoConfigMerge.Merge(files.ReadText(videoPath), tweaks.Video);
                if (video.Changed)
                {
                    files.WriteText(videoPath, video.Text!);
                }
            }

            var changed = gameInfo.Changed || video?.Changed == true;
            return new ApplyOutcome(changed ? ApplyResult.Applied : ApplyResult.UpToDate, gameInfo, video);
        }
        catch (GameFileException exception)
        {
            var result = exception.IsLocked ? ApplyResult.Waiting : ApplyResult.Failed;
            return new ApplyOutcome(result, gameInfo, video, exception.Message);
        }
    }

    /// <summary>The video settings file that sits beside gameinfo.gi, in its cfg folder.</summary>
    public static string VideoConfigPathOf(string gameInfoPath) =>
        Path.Combine(Path.GetDirectoryName(gameInfoPath) ?? string.Empty, "cfg", "video.txt");
}
