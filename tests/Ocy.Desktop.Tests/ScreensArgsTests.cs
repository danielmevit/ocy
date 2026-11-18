using Ocy.Desktop.Layout;
using Xunit;

namespace Ocy.Desktop.Tests;

public sealed class ScreensArgsTests
{
    [Fact]
    public void IsRequested_ReturnsTrue_WhenScreensFlagPresent()
    {
        Assert.True(ScreensArgs.IsRequested(["--screens"]));
        Assert.True(ScreensArgs.IsRequested(["--screens=out.json"]));
        Assert.True(ScreensArgs.IsRequested(["--foo", "--screens", "out.json"]));
    }

    [Fact]
    public void IsRequested_ReturnsFalse_WhenScreensFlagMissing()
    {
        Assert.False(ScreensArgs.IsRequested([]));
        Assert.False(ScreensArgs.IsRequested(["--smoke"]));
        Assert.False(ScreensArgs.IsRequested(["--other"]));
    }

    [Fact]
    public void ParseOutFile_ReturnsExpectedPath()
    {
        Assert.Equal("out.json", ScreensArgs.ParseOutFile(["--screens", "out.json"]));
        Assert.Equal("my screens.json", ScreensArgs.ParseOutFile(["--screens=\"my screens.json\""]));
        Assert.Null(ScreensArgs.ParseOutFile(["--screens"]));
        Assert.Null(ScreensArgs.ParseOutFile(["--screens", "--other"]));
    }
}
