import Quartz
import sys

def get_pdf_text(path):
    url = Quartz.NSURL.fileURLWithPath_(path)
    pdf = Quartz.CGPDFDocumentCreateWithURL(url)
    if not pdf:
        print("Failed to open PDF")
        return
    page = Quartz.CGPDFDocumentGetPage(pdf, 1)
    if not page:
        print("No page 1")
        return
    # This is a bit complex in pure Quartz, let's just use simple python if possible
