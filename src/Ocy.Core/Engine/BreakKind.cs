using System.Diagnostics.CodeAnalysis;

namespace Ocy.Core.Engine;

[SuppressMessage("Naming", "CA1720:Identifiers should not contain type names", Justification = "Short and Long represent break duration concepts")]
public enum BreakKind
{
    Short = 1,
    Long = 2,
}
