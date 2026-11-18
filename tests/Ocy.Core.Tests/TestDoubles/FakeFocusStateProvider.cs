using Ocy.Abstractions;

namespace Ocy.Core.Tests.TestDoubles;

public sealed class FakeFocusStateProvider : IFocusStateProvider
{
    public bool IsSupported { get; set; } = true;

    public FocusState State { get; set; } = FocusState.Normal;

    public FocusState GetState() => IsSupported ? State : FocusState.Unknown;
}
