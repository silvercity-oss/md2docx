// Validates .docx files against the Office Open XML schemas with Microsoft's Open XML SDK
// validator, which is much closer to Word's own strictness than a round trip through LibreOffice.
//
//   dotnet run --project tools/ooxml-validate -- file.docx [more.docx ...]
//
// Exit status: 0 = every file is valid, 1 = at least one schema error, 2 = a file would not open.
using DocumentFormat.OpenXml;
using DocumentFormat.OpenXml.Packaging;
using DocumentFormat.OpenXml.Validation;

var failed = 0;
foreach (var path in args)
{
    try
    {
        using var doc = WordprocessingDocument.Open(path, false);
        var errors = new OpenXmlValidator(FileFormatVersions.Office2019).Validate(doc).ToList();
        if (errors.Count == 0)
        {
            Console.WriteLine($"ok       {path}");
            continue;
        }
        failed = Math.Max(failed, 1);
        Console.WriteLine($"INVALID  {path}: {errors.Count} error(s)");
        foreach (var e in errors.Take(25))
            Console.WriteLine($"    [{e.ErrorType}] {e.Part?.Uri} {e.Path?.XPath}: {e.Description}");
    }
    catch (Exception ex)
    {
        Console.WriteLine($"UNOPENABLE  {path}: {ex.Message}");
        failed = 2;
    }
}
return failed;
