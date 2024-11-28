Transforms KDL to HTML according to [this spec](spec).  Also supports:

* Markdown nodes
* Include files (HTML, KDL, markdown)
* String interpolation
* Set variables with the CLI or with an envfile

[spec]: https://github.com/kdl-org/kdl/blob/main/XML-IN-KDL.md

## Sample

navbar.kdl:

```kdl
nav {
    a href="index.html" "Home"
    a href="about.html" "About"
}
```

news.md:

```markdown
# News

Nothing going on!
```

template.kdl:

```kdl
!doctype "html"
html lang="en" {
    head {
        meta charset="utf-8"
        title "${TITLE} - AwesomeSite"
        link href="/style.css" rel="stylesheet"
        script src="/script.js"
    }
    body {
        div class="container" {
            header {
                div class="logo" {
                    img src="logo.png"
                    - "AwesomeSite"
                }
            }
            @include "navbar.kdl"
            main {
                markdown {
                    "Welcome to AwesomeSite, the _craziest_ site"
                    "on the entire 'net since ${YEAR}!"
                }
                @include "news.md"
            }
            footer "© ${YEAR} AwesomeSite"
        }
    }
}
```

Comile the page like this:

```console
$ kdl2html template.kdl --bind "TITLE=home" --bind "YEAR=1999"
```

Here's what comes out:

```html
<!DOCTYPE html>
<html lang="en">
	<head>
		<meta charset="utf-8" />
		<title>Home - AwesomeSite</title>
		<link href="/style.css" rel="stylesheet" />
        <script src="/script.js"></script>
	</head>
	<body>
		<div class="container">
			<header>
				<div class="logo">
					<img src="logo.png" />
					AwesomeSite
				</div>
			</header>
			<nav>
				<a href="index.html">Home</a>
				<a href="about.html">About</a>
			</nav>
			<main>
				<p>Welcome to AwesomeSite, the <em>craziest</em> site
				on the entire 'net since 1999!</p>
				<h1>News</h1>
				<p>Nothing going on!</p>
			</main>
			<footer>© 1999 AwesomeSite</footer>
		</div>
	</body>
</html>
```
