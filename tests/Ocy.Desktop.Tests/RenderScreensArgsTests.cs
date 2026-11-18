using Ocy.Desktop.Layout;
using Xunit;

namespace Ocy.Desktop.Tests;

public sealed class RenderScreensArgsTests
{
    [Fact]
    public void IsRequested_ReturnsTrue_WhenRenderScreensFlagPresent()
    {
        Assert.True(RenderScreensArgs.IsRequested(["--render-screens"]));
        Assert.True(RenderScreensArgs.IsRequested(["--render-screens=out/screens"]));
        Assert.True(RenderScreensArgs.IsRequested(["--foo", "--render-screens", "out/screens"]));
    }

    [Fact]
    public void IsRequested_ReturnsFalse_WhenFlagMissing()
    {
        Assert.False(RenderScreensArgs.IsRequested([]));
        Assert.False(RenderScreensArgs.IsRequested(["--smoke"]));
        Assert.False(RenderScreensArgs.IsRequested(["--screens"]));
    }

    [Fact]
    public void ParseDirectory_SupportsEqualsSeparateAndQuotedPaths()
    {
        Assert.Equal("out/screens", RenderScreensArgs.ParseDirectory(["--render-screens=out/screens"]));
        Assert.Equal("my screens", RenderScreensArgs.ParseDirectory(["--render-screens", "my screens"]));
        Assert.Equal("my screens", RenderScreensArgs.ParseDirectory(["--render-screens=\"my screens\""]));
        Assert.Null(RenderScreensArgs.ParseDirectory(["--render-screens"]));
        Assert.Null(RenderScreensArgs.ParseDirectory(["--render-screens", "--other"]));
    }
}
