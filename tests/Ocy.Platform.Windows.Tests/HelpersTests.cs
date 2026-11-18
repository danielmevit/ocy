using Ocy.Abstractions;
using Ocy.Platform.Windows;
using Xunit;

namespace Ocy.Platform.Windows.Tests;

public class MotionMathTests
{
    [Theory]
    [InlineData(0)]
    [InlineData(1)]
    [InlineData(7)]
    public void Reduced_CallFailed_ReturnsFalse(int clientAreaAnimation)
    {
        Assert.False(MotionMath.Reduced(false, clientAreaAnimation));
    }

    [Fact]
    public void Reduced_ValueZero_ReturnsTrue()
    {
        Assert.True(MotionMath.Reduced(true, 0));
    }

    [Theory]
    [InlineData(1)]
    [InlineData(2)]
    [InlineData(42)]
    public void Reduced_ValueNonZero_ReturnsFalse(int clientAreaAnimation)
    {
        Assert.False(MotionMath.Reduced(true, clientAreaAnimation));
    }
}

public class TaskbarThemeMathTests
{
    [Fact]
    public void FromRegistryValue_One_ReturnsTrue()
    {
        Assert.Equal(true, TaskbarThemeMath.FromRegistryValue(1));
    }

    [Fact]
    public void FromRegistryValue_Zero_ReturnsFalse()
    {
        Assert.Equal(false, TaskbarThemeMath.FromRegistryValue(0));
    }

    [Fact]
    public void FromRegistryValue_Null_ReturnsNull()
    {
        Assert.Null(TaskbarThemeMath.FromRegistryValue(null));
    }

    [Theory]
    [InlineData(2)]
    [InlineData(-1)]
    [InlineData(int.MaxValue)]
    public void FromRegistryValue_OtherInt_ReturnsNull(int value)
    {
        Assert.Null(TaskbarThemeMath.FromRegistryValue(value));
    }

    [Theory]
    [InlineData("1")]
    [InlineData(1L)]
    [InlineData((short)1)]
    [InlineData(true)]
    [InlineData(1.0)]
    public void FromRegistryValue_OtherType_ReturnsNull(object value)
    {
        Assert.Null(TaskbarThemeMath.FromRegistryValue(value));
    }
}

public class IdleMathTests
{
    [Fact]
    public void Compute_NowAfterLastInput_ReturnsDifference()
    {
        Assert.Equal(TimeSpan.FromMilliseconds(12_345), IdleMath.Compute(12_345u, 0u));
    }

    [Fact]
    public void Compute_SameTicks_ReturnsZero()
    {
        Assert.Equal(TimeSpan.Zero, IdleMath.Compute(100_000u, 100_000u));
    }

    [Fact]
    public void Compute_Wraparound_IsSafe()
    {
        uint now = 5u;
        uint lastInput = uint.MaxValue - 100u;
        Assert.Equal(TimeSpan.FromMilliseconds(106), IdleMath.Compute(now, lastInput));
    }

    [Fact]
    public void Compute_MaxSpan_Wraparound_IsSafe()
    {
        uint lastInput = 1u;
        Assert.Equal(TimeSpan.FromMilliseconds(uint.MaxValue), IdleMath.Compute(0u, lastInput));
    }
}

public class FocusStateMapperTests
{
    [Theory]
    [InlineData(1, FocusState.NotPresent)]
    [InlineData(2, FocusState.FullscreenApp)]
    [InlineData(3, FocusState.FullscreenApp)]
    [InlineData(4, FocusState.Presentation)]
    [InlineData(5, FocusState.Normal)]
    [InlineData(6, FocusState.QuietTime)]
    [InlineData(7, FocusState.FullscreenApp)]
    public void Map_SuccessfulHresult_MapsEachState(int quns, FocusState expected)
    {
        Assert.Equal(expected, FocusStateMapper.Map(0, quns));
    }

    [Theory]
    [InlineData(-2147467259)]
    [InlineData(-1)]
    public void Map_FailedHresult_ReturnsUnknown(int hresult)
    {
        Assert.Equal(FocusState.Unknown, FocusStateMapper.Map(hresult, 5));
    }

    [Theory]
    [InlineData(0)]
    [InlineData(8)]
    [InlineData(99)]
    public void Map_UnknownQuns_ReturnsUnknown(int quns)
    {
        Assert.Equal(FocusState.Unknown, FocusStateMapper.Map(0, quns));
    }
}

public class AutostartCommandTests
{
    [Fact]
    public void Build_QuotesPath()
    {
        Assert.Equal("\"C:\\Program Files\\Ocy\\Ocy.exe\" --autostart",
            AutostartCommand.Build("C:\\Program Files\\Ocy\\Ocy.exe"));
    }

    [Fact]
    public void Build_SimplePath()
    {
        Assert.Equal("\"C:\\Ocy\\Ocy.exe\" --autostart", AutostartCommand.Build("C:\\Ocy\\Ocy.exe"));
    }

    [Theory]
    [InlineData("")]
    [InlineData("   ")]
    public void Build_EmptyPath_Throws(string path)
    {
        Assert.Throws<ArgumentException>(() => AutostartCommand.Build(path));
    }

    [Fact]
    public void Build_NullPath_Throws()
    {
        Assert.Throws<ArgumentNullException>(() => AutostartCommand.Build(null!));
    }

    [Fact]
    public void Build_PathWithQuote_Throws()
    {
        Assert.Throws<ArgumentException>(() => AutostartCommand.Build("C:\\O\"cy\\Ocy.exe"));
    }
}

public class SessionEventMapperTests
{
    [Fact]
    public void FromSessionSwitch_Lock_ReturnsLocked()
    {
        Assert.Equal(SessionEventKind.Locked, SessionEventMapper.FromSessionSwitch(7));
    }

    [Fact]
    public void FromSessionSwitch_Unlock_ReturnsUnlocked()
    {
        Assert.Equal(SessionEventKind.Unlocked, SessionEventMapper.FromSessionSwitch(8));
    }

    [Fact]
    public void FromSessionSwitch_Logoff_ReturnsLoggingOff()
    {
        Assert.Equal(SessionEventKind.LoggingOff, SessionEventMapper.FromSessionSwitch(6));
    }

    [Theory]
    [InlineData(0)]
    [InlineData(1)]
    [InlineData(5)]
    [InlineData(9)]
    [InlineData(11)]
    public void FromSessionSwitch_UnmappedReason_ReturnsNull(int reason)
    {
        Assert.Null(SessionEventMapper.FromSessionSwitch(reason));
    }

    [Fact]
    public void FromPowerMode_Resume_ReturnsResumed()
    {
        Assert.Equal(SessionEventKind.Resumed, SessionEventMapper.FromPowerMode(1));
    }

    [Fact]
    public void FromPowerMode_Suspend_ReturnsSuspending()
    {
        Assert.Equal(SessionEventKind.Suspending, SessionEventMapper.FromPowerMode(3));
    }

    [Theory]
    [InlineData(0)]
    [InlineData(2)]
    [InlineData(4)]
    public void FromPowerMode_UnmappedMode_ReturnsNull(int mode)
    {
        Assert.Null(SessionEventMapper.FromPowerMode(mode));
    }
}
