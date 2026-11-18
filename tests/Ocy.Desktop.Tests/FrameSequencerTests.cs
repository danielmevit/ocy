using Ocy.Desktop.Illustrations;
using Xunit;

namespace Ocy.Desktop.Tests;

public sealed class FrameSequencerTests
{
    [Fact]
    public void SequenceOrder_AdvancesThroughSequence()
    {
        var sequence = new[] { 1, 2, 3 };
        var step = TimeSpan.FromMilliseconds(100);
        var sequencer = new FrameSequencer(sequence, step);

        Assert.Equal(1, sequencer.GetFrame(TimeSpan.Zero));
        Assert.Equal(1, sequencer.GetFrame(TimeSpan.FromMilliseconds(50)));
        Assert.Equal(2, sequencer.GetFrame(TimeSpan.FromMilliseconds(100)));
        Assert.Equal(2, sequencer.GetFrame(TimeSpan.FromMilliseconds(150)));
        Assert.Equal(3, sequencer.GetFrame(TimeSpan.FromMilliseconds(200)));
        Assert.Equal(3, sequencer.GetFrame(TimeSpan.FromMilliseconds(299)));
    }

    [Fact]
    public void Looping_LoopsBackToStartOfSequence()
    {
        var sequence = new[] { 1, 2, 3 };
        var step = TimeSpan.FromMilliseconds(100);

        // Sequence total duration = 300 ms
        Assert.Equal(1, FrameSequencer.GetFrame(sequence, step, TimeSpan.FromMilliseconds(300)));
        Assert.Equal(2, FrameSequencer.GetFrame(sequence, step, TimeSpan.FromMilliseconds(400)));
        Assert.Equal(3, FrameSequencer.GetFrame(sequence, step, TimeSpan.FromMilliseconds(500)));
        Assert.Equal(1, FrameSequencer.GetFrame(sequence, step, TimeSpan.FromMilliseconds(600)));
        Assert.Equal(2, FrameSequencer.GetFrame(sequence, step, TimeSpan.FromMilliseconds(1000))); // 10 steps = step index 10 % 3 = 1 -> sequence[1] = 2
    }

    [Fact]
    public void Holds_HoldsFramesWithNoInterpolation()
    {
        // Real manifest sequence for blink: [1, 1, 1, 1, 1, 2, 3, 3, 2], stepMs: 280
        var sequence = new[] { 1, 1, 1, 1, 1, 2, 3, 3, 2 };
        var step = TimeSpan.FromMilliseconds(280);

        // Frame 1 should be held for 5 steps (0 to 5 * 280 - 1 = 1399 ms)
        for (var ms = 0; ms < 1400; ms += 70)
        {
            Assert.Equal(1, FrameSequencer.GetFrame(sequence, step, TimeSpan.FromMilliseconds(ms)));
        }

        // Step 5 (1400 ms) is frame 2
        Assert.Equal(2, FrameSequencer.GetFrame(sequence, step, TimeSpan.FromMilliseconds(1400)));
        Assert.Equal(2, FrameSequencer.GetFrame(sequence, step, TimeSpan.FromMilliseconds(1600)));

        // Step 6 & 7 (1680 ms - 2239 ms) is frame 3 held for 2 steps
        Assert.Equal(3, FrameSequencer.GetFrame(sequence, step, TimeSpan.FromMilliseconds(1680)));
        Assert.Equal(3, FrameSequencer.GetFrame(sequence, step, TimeSpan.FromMilliseconds(2000)));

        // Step 8 (2240 ms) is frame 2
        Assert.Equal(2, FrameSequencer.GetFrame(sequence, step, TimeSpan.FromMilliseconds(2240)));

        // Step 9 (2520 ms) loops back to frame 1
        Assert.Equal(1, FrameSequencer.GetFrame(sequence, step, TimeSpan.FromMilliseconds(2520)));
    }

    [Fact]
    public void EdgeCases_HandleNegativeElapsedAndZeroStep()
    {
        var sequence = new[] { 1, 2, 3 };
        var step = TimeSpan.FromMilliseconds(100);

        // Negative elapsed clamps to 0 (first frame)
        Assert.Equal(1, FrameSequencer.GetFrame(sequence, step, TimeSpan.FromMilliseconds(-50)));

        // Zero or negative step returns first frame
        Assert.Equal(1, FrameSequencer.GetFrame(sequence, TimeSpan.Zero, TimeSpan.FromSeconds(5)));
        Assert.Equal(1, FrameSequencer.GetFrame(sequence, TimeSpan.FromSeconds(-1), TimeSpan.FromSeconds(5)));
    }

    [Fact]
    public void InvalidArguments_ThrowExpectedExceptions()
    {
        Assert.Throws<ArgumentNullException>(() => FrameSequencer.GetFrame(null!, TimeSpan.FromSeconds(1), TimeSpan.Zero));
        Assert.Throws<ArgumentException>(() => FrameSequencer.GetFrame(Array.Empty<int>(), TimeSpan.FromSeconds(1), TimeSpan.Zero));
    }
}
