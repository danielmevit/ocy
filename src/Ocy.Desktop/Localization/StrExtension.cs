using Avalonia.Markup.Xaml;

namespace Ocy.Desktop.Localization;

/// <summary>
/// XAML markup extension for UI text: <c>{loc:Str settings.section.general}</c> returns the string for that key
/// from <c>content/strings/en.json</c> (via <see cref="Strings.Shared"/>).
/// </summary>
public sealed class StrExtension : MarkupExtension
{
    public StrExtension()
    {
    }

    public StrExtension(string key)
    {
        Key = key;
    }

    public string Key { get; set; } = string.Empty;

    public override object ProvideValue(IServiceProvider serviceProvider) => Strings.Shared.Get(Key);
}
