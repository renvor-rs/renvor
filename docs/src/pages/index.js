import clsx from 'clsx';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';
import styles from './index.module.css';

function HomepageHeader() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <header className={clsx('hero hero--primary', styles.heroBanner)}>
      <div className="container">
        <Heading as="h1" className="hero__title">
          {siteConfig.title}
        </Heading>
        <p className="hero__subtitle">{siteConfig.tagline}</p>
        <div className={styles.buttons}>
          <Link className="button button--secondary button--lg" to="/docs/intro">
            Read the documentation
          </Link>
        </div>
      </div>
    </header>
  );
}

export default function Home() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <Layout
      title={siteConfig.title}
      description="Renvor is a Rust framework. It is pre-release and ships no runtime capability yet.">
      <HomepageHeader />
      <main>
        <div className="container margin-vert--lg">
          <div className="row">
            <div className="col col--8 col--offset-2">
              <h2>Renvor does not work yet</h2>
              <p>
                The <code>renvor</code> crate exposes three constants and nothing else —
                and it is <strong>not published</strong>, so there is no way to install
                it. What exists today is the project foundation — governance,
                verified names, a pinned toolchain, a licence policy, and a fail-closed
                verification sequence — built before any code, so the first line of
                functionality lands into a project that can already verify itself.
              </p>
              <p>
                <strong>Do not adopt Renvor for anything yet.</strong> Nothing is stable,
                and no compatibility promise applies before a <code>0.1.0</code> release.
              </p>
              <h2>The command is <code>renover</code></h2>
              <p>
                The product is <strong>Renvor</strong>. The installed executable is{' '}
                <strong><code>renover</code></strong>. The difference is deliberate and
                permanent — it is not a typographical error.
              </p>
            </div>
          </div>
        </div>
      </main>
    </Layout>
  );
}
